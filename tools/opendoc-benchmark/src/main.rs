#![forbid(unsafe_code)]
#![allow(clippy::print_stderr, clippy::print_stdout)]

#[cfg(target_arch = "wasm32")]
fn main() {
    println!("OpenDoc benchmark execution is native-only");
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::{BTreeMap, BTreeSet};
    use std::env;
    use std::error::Error;
    use std::fmt;
    use std::fs::{self, OpenOptions};
    use std::hint::black_box;
    use std::io::Write;
    use std::path::{Component, Path, PathBuf};
    use std::process::Command;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    use casual_doc_ooxml::{DocxPackage, PackageLimits};
    use casual_doc_sdk::{
        Affinity, BlockSnapshot, Engine, EngineConfig, InlineSnapshot, InsertTextRequest,
        OpenNormalizedOptions, Position,
    };
    use serde::{Deserialize, Serialize};

    const REPORT_SCHEMA_VERSION: u32 = 1;
    const MINIMAL_DOCX: &[u8] = include_bytes!("../../../fixtures/generated/minimal-valid.docx");
    const DOCUMENT_XML: &[u8] = br#"<?xml version="1.0"?><w:document/>"#;
    const DOCUMENT_PART: &str = "word/document.xml";
    const DEFAULT_MAX_REGRESSION_BASIS_POINTS: u32 = 2_000;
    const FULL_WARMUP_SAMPLES: u32 = 3;
    const FULL_MEASURED_SAMPLES: u32 = 15;
    const SMOKE_WARMUP_SAMPLES: u32 = 1;
    const SMOKE_MEASURED_SAMPLES: u32 = 2;

    pub(super) fn main() {
        match Config::parse(env::args().skip(1)) {
            Ok(ParseOutcome::Run(config)) => {
                if let Err(error) = execute(config) {
                    eprintln!("benchmark failed: {error}");
                    std::process::exit(1);
                }
            }
            Ok(ParseOutcome::Help) => {
                println!("{HELP}");
            }
            Err(error) => {
                eprintln!("benchmark configuration failed: {error}");
                eprintln!("{HELP}");
                std::process::exit(2);
            }
        }
    }

    const HELP: &str = "\
OpenDoc benchmark runner

USAGE:
  opendoc-benchmark --smoke --output <path>
  opendoc-benchmark --environment-id <id> --source-revision <sha> \
--source-state <clean|dirty> --output <path>
  opendoc-benchmark ... --compare <baseline> [--max-regression-percent <value>]";

    #[derive(Debug)]
    struct AppError(String);

    impl AppError {
        fn new(message: impl Into<String>) -> Self {
            Self(message.into())
        }
    }

    impl fmt::Display for AppError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(&self.0)
        }
    }

    impl Error for AppError {}

    #[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
    #[serde(rename_all = "snake_case")]
    enum RunMode {
        Smoke,
        Full,
    }

    #[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
    #[serde(rename_all = "snake_case")]
    enum SourceState {
        Clean,
        Dirty,
        Unknown,
    }

    impl SourceState {
        fn parse(value: &str) -> Result<Self, AppError> {
            match value {
                "clean" => Ok(Self::Clean),
                "dirty" => Ok(Self::Dirty),
                "unknown" => Ok(Self::Unknown),
                _ => Err(AppError::new(
                    "--source-state must be clean, dirty, or unknown",
                )),
            }
        }
    }

    #[derive(Debug)]
    enum ParseOutcome {
        Run(Config),
        Help,
    }

    #[derive(Debug)]
    struct Config {
        mode: RunMode,
        output: PathBuf,
        environment_id: String,
        source_revision: String,
        source_state: SourceState,
        compare: Option<PathBuf>,
        max_regression_basis_points: Option<u32>,
    }

    impl Config {
        fn parse(arguments: impl Iterator<Item = String>) -> Result<ParseOutcome, AppError> {
            let mut smoke = false;
            let mut output = None;
            let mut environment_id = None;
            let mut source_revision = None;
            let mut source_state = None;
            let mut compare = None;
            let mut max_regression_basis_points = None;
            let mut seen = BTreeSet::new();
            let mut arguments = arguments.peekable();

            while let Some(argument) = arguments.next() {
                if argument == "--help" || argument == "-h" {
                    if arguments.peek().is_some() || !seen.is_empty() || smoke {
                        return Err(AppError::new(
                            "--help cannot be combined with other options",
                        ));
                    }
                    return Ok(ParseOutcome::Help);
                }
                if argument == "--smoke" {
                    if smoke {
                        return Err(AppError::new("duplicate option --smoke"));
                    }
                    smoke = true;
                    continue;
                }

                let key = match argument.as_str() {
                    "--output"
                    | "--environment-id"
                    | "--source-revision"
                    | "--source-state"
                    | "--compare"
                    | "--max-regression-percent" => argument,
                    _ => return Err(AppError::new(format!("unknown option {argument}"))),
                };
                if !seen.insert(key.clone()) {
                    return Err(AppError::new(format!("duplicate option {key}")));
                }
                let value = arguments
                    .next()
                    .ok_or_else(|| AppError::new(format!("missing value for {key}")))?;
                if value.starts_with('-') {
                    return Err(AppError::new(format!("missing value for {key}")));
                }
                match key.as_str() {
                    "--output" => output = Some(PathBuf::from(value)),
                    "--environment-id" => environment_id = Some(validate_identifier(&value)?),
                    "--source-revision" => source_revision = Some(validate_revision(&value)?),
                    "--source-state" => source_state = Some(SourceState::parse(&value)?),
                    "--compare" => compare = Some(PathBuf::from(value)),
                    "--max-regression-percent" => {
                        max_regression_basis_points = Some(parse_percent_basis_points(&value)?);
                    }
                    _ => unreachable!("recognized option"),
                }
            }

            let mode = if smoke { RunMode::Smoke } else { RunMode::Full };
            let output = output.ok_or_else(|| AppError::new("--output is required"))?;
            validate_json_path(&output, "output")?;

            if let Some(path) = &compare {
                validate_json_path(path, "baseline")?;
            }
            if max_regression_basis_points.is_some() && compare.is_none() {
                return Err(AppError::new("--max-regression-percent requires --compare"));
            }

            let (environment_id, source_revision, source_state) = match mode {
                RunMode::Smoke => {
                    if compare.is_some() {
                        return Err(AppError::new("--compare is unavailable in smoke mode"));
                    }
                    (
                        environment_id.unwrap_or_else(|| "ci-smoke".to_owned()),
                        source_revision
                            .or_else(|| env::var("OPENDOC_SOURCE_REVISION").ok())
                            .or_else(|| env::var("GITHUB_SHA").ok())
                            .unwrap_or_else(|| "unknown".to_owned()),
                        source_state.unwrap_or(SourceState::Unknown),
                    )
                }
                RunMode::Full => {
                    let source_state = source_state
                        .ok_or_else(|| AppError::new("--source-state is required in full mode"))?;
                    if source_state == SourceState::Unknown {
                        return Err(AppError::new(
                            "--source-state must be clean or dirty in full mode",
                        ));
                    }
                    (
                        environment_id.ok_or_else(|| {
                            AppError::new("--environment-id is required in full mode")
                        })?,
                        source_revision.ok_or_else(|| {
                            AppError::new("--source-revision is required in full mode")
                        })?,
                        source_state,
                    )
                }
            };

            Ok(ParseOutcome::Run(Self {
                mode,
                output,
                environment_id,
                source_revision,
                source_state,
                compare,
                max_regression_basis_points,
            }))
        }
    }

    fn validate_identifier(value: &str) -> Result<String, AppError> {
        if value.is_empty()
            || value.len() > 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(AppError::new(
                "environment ID must contain 1-64 ASCII letters, digits, dots, dashes, or underscores",
            ));
        }
        Ok(value.to_owned())
    }

    fn validate_revision(value: &str) -> Result<String, AppError> {
        if value.is_empty()
            || value.len() > 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(AppError::new(
                "source revision must contain 1-64 safe ASCII identifier characters",
            ));
        }
        Ok(value.to_owned())
    }

    fn validate_json_path(path: &Path, kind: &str) -> Result<(), AppError> {
        if path.as_os_str().is_empty()
            || path.extension().and_then(|value| value.to_str()) != Some("json")
            || path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(AppError::new(format!(
                "{kind} path must be a .json path without parent traversal"
            )));
        }
        Ok(())
    }

    fn parse_percent_basis_points(value: &str) -> Result<u32, AppError> {
        let percent = value.parse::<f64>().map_err(|_| {
            AppError::new("--max-regression-percent must be a finite number from 0 to 100")
        })?;
        if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
            return Err(AppError::new(
                "--max-regression-percent must be a finite number from 0 to 100",
            ));
        }
        let basis_points = (percent * 100.0).round();
        if basis_points > f64::from(u32::MAX) {
            return Err(AppError::new("regression percentage is too large"));
        }
        Ok(basis_points as u32)
    }

    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct BenchmarkReport {
        schema_version: u32,
        runner_version: String,
        source_revision: String,
        source_state: SourceState,
        generated_at_unix_seconds: u64,
        build_profile: String,
        environment: Environment,
        mode: RunMode,
        cases: Vec<CaseReport>,
    }

    #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Environment {
        id: String,
        operating_system: String,
        architecture: String,
        rust_version: String,
        logical_cpus: usize,
    }

    #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct CaseReport {
        id: String,
        work_units: u64,
        warmup_samples: u32,
        measured_samples: u32,
        iterations_per_sample: u32,
        output_checksum: String,
        minimum_nanoseconds_per_iteration: u64,
        median_nanoseconds_per_iteration: u64,
        p95_nanoseconds_per_iteration: u64,
        max_regression_basis_points: u32,
        absolute_noise_nanoseconds: u64,
    }

    struct BenchmarkDefinition {
        id: &'static str,
        work_units: u64,
        full_iterations: u32,
        expected_unit_checksum: u64,
        absolute_noise_nanoseconds: u64,
        execute: fn(&BenchmarkInputs) -> Result<u64, AppError>,
    }

    struct BenchmarkInputs {
        normalized_json: Vec<u8>,
        engine: Engine,
    }

    fn execute(config: Config) -> Result<(), AppError> {
        if config.mode == RunMode::Full && cfg!(debug_assertions) {
            return Err(AppError::new(
                "full benchmarks require a release build; use cargo run --release",
            ));
        }

        let report = run_benchmarks(&config)?;
        write_report_atomically(&config.output, &report)?;
        println!("wrote benchmark report {}", config.output.display());
        print_summary(&report);

        if let Some(baseline_path) = &config.compare {
            let baseline = read_report(baseline_path)?;
            compare_reports(&report, &baseline, config.max_regression_basis_points)?;
            println!("baseline comparison passed {}", baseline_path.display());
        }
        Ok(())
    }

    fn run_benchmarks(config: &Config) -> Result<BenchmarkReport, AppError> {
        let inputs = BenchmarkInputs {
            normalized_json: normalized_document_json()?,
            engine: Engine::new(EngineConfig {
                id_namespace: 0x4f50_454e_444f_4300,
            })
            .map_err(|error| AppError::new(format!("benchmark engine setup failed: {error}")))?,
        };
        let definitions = benchmark_definitions();
        let mut cases = Vec::with_capacity(definitions.len());
        for definition in definitions {
            cases.push(run_case(&definition, &inputs, config.mode)?);
        }

        Ok(BenchmarkReport {
            schema_version: REPORT_SCHEMA_VERSION,
            runner_version: env!("CARGO_PKG_VERSION").to_owned(),
            source_revision: config.source_revision.clone(),
            source_state: config.source_state,
            generated_at_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| AppError::new("system clock is before the Unix epoch"))?
                .as_secs(),
            build_profile: if cfg!(debug_assertions) {
                "debug".to_owned()
            } else {
                "release".to_owned()
            },
            environment: Environment {
                id: config.environment_id.clone(),
                operating_system: env::consts::OS.to_owned(),
                architecture: env::consts::ARCH.to_owned(),
                rust_version: rust_version()?,
                logical_cpus: std::thread::available_parallelism()
                    .map_err(|error| {
                        AppError::new(format!("logical CPU detection failed: {error}"))
                    })?
                    .get(),
            },
            mode: config.mode,
            cases,
        })
    }

    fn benchmark_definitions() -> [BenchmarkDefinition; 4] {
        [
            BenchmarkDefinition {
                id: "docx.package_open.minimal",
                work_units: 3,
                full_iterations: 500,
                expected_unit_checksum: (3_u64 << 32) | 100,
                absolute_noise_nanoseconds: 10_000,
                execute: package_open_minimal,
            },
            BenchmarkDefinition {
                id: "docx.part_read.document_xml",
                work_units: u64::try_from(DOCUMENT_XML.len()).unwrap_or(u64::MAX),
                full_iterations: 500,
                expected_unit_checksum: fnv1a(DOCUMENT_XML),
                absolute_noise_nanoseconds: 10_000,
                execute: package_read_document,
            },
            BenchmarkDefinition {
                id: "model.normalized_load.100_paragraphs",
                work_units: 100,
                full_iterations: 100,
                expected_unit_checksum: 100,
                absolute_noise_nanoseconds: 50_000,
                execute: normalized_load,
            },
            BenchmarkDefinition {
                id: "sdk.typing.100_graphemes",
                work_units: 100,
                full_iterations: 5,
                expected_unit_checksum: (100_u64 << 32) | 100,
                absolute_noise_nanoseconds: 500_000,
                execute: typing_100_graphemes,
            },
        ]
    }

    fn run_case(
        definition: &BenchmarkDefinition,
        inputs: &BenchmarkInputs,
        mode: RunMode,
    ) -> Result<CaseReport, AppError> {
        let (warmup_samples, measured_samples, iterations) = match mode {
            RunMode::Smoke => (SMOKE_WARMUP_SAMPLES, SMOKE_MEASURED_SAMPLES, 1),
            RunMode::Full => (
                FULL_WARMUP_SAMPLES,
                FULL_MEASURED_SAMPLES,
                definition.full_iterations,
            ),
        };
        let expected_checksum = accumulated_checksum(definition.expected_unit_checksum, iterations);

        for _ in 0..warmup_samples {
            let (_, checksum) = measure_sample(definition, inputs, iterations)?;
            validate_case_checksum(definition.id, checksum, expected_checksum)?;
        }

        let mut samples = Vec::with_capacity(measured_samples as usize);
        for _ in 0..measured_samples {
            let (nanoseconds, checksum) = measure_sample(definition, inputs, iterations)?;
            validate_case_checksum(definition.id, checksum, expected_checksum)?;
            samples.push(nanoseconds);
        }
        samples.sort_unstable();

        Ok(CaseReport {
            id: definition.id.to_owned(),
            work_units: definition.work_units,
            warmup_samples,
            measured_samples,
            iterations_per_sample: iterations,
            output_checksum: format!("{expected_checksum:016x}"),
            minimum_nanoseconds_per_iteration: samples[0],
            median_nanoseconds_per_iteration: median(&samples)?,
            p95_nanoseconds_per_iteration: percentile_nearest_rank(&samples, 95)?,
            max_regression_basis_points: DEFAULT_MAX_REGRESSION_BASIS_POINTS,
            absolute_noise_nanoseconds: definition.absolute_noise_nanoseconds,
        })
    }

    fn measure_sample(
        definition: &BenchmarkDefinition,
        inputs: &BenchmarkInputs,
        iterations: u32,
    ) -> Result<(u64, u64), AppError> {
        let mut checksum = 0_u64;
        let started = Instant::now();
        for _ in 0..iterations {
            let output = black_box((definition.execute)(black_box(inputs))?);
            checksum = fold_checksum(checksum, output);
        }
        let elapsed = started.elapsed().as_nanos();
        if elapsed == 0 {
            return Err(AppError::new(format!(
                "workload {} completed with zero elapsed time",
                definition.id
            )));
        }
        let per_iteration = elapsed / u128::from(iterations);
        let per_iteration = u64::try_from(per_iteration)
            .map_err(|_| AppError::new(format!("workload {} timing overflowed", definition.id)))?;
        Ok((per_iteration.max(1), checksum))
    }

    fn validate_case_checksum(id: &str, observed: u64, expected: u64) -> Result<(), AppError> {
        if observed != expected {
            return Err(AppError::new(format!(
                "workload {id} output checksum mismatch: expected {expected:016x}, observed {observed:016x}"
            )));
        }
        Ok(())
    }

    const fn fold_checksum(current: u64, value: u64) -> u64 {
        current.rotate_left(7) ^ value.wrapping_mul(0x9e37_79b1_85eb_ca87)
    }

    fn accumulated_checksum(unit: u64, iterations: u32) -> u64 {
        let mut checksum = 0_u64;
        for _ in 0..iterations {
            checksum = fold_checksum(checksum, unit);
        }
        checksum
    }

    fn package_open_minimal(_: &BenchmarkInputs) -> Result<u64, AppError> {
        let package = DocxPackage::open(MINIMAL_DOCX, PackageLimits::default())
            .map_err(|error| AppError::new(format!("minimal package admission failed: {error}")))?;
        Ok(
            (u64::try_from(package.entries().len()).unwrap_or(u64::MAX) << 32)
                | package.total_expanded_bytes(),
        )
    }

    fn package_read_document(_: &BenchmarkInputs) -> Result<u64, AppError> {
        let mut package = DocxPackage::open(MINIMAL_DOCX, PackageLimits::default())
            .map_err(|error| AppError::new(format!("minimal package admission failed: {error}")))?;
        let bytes = package
            .read_part(DOCUMENT_PART)
            .map_err(|error| AppError::new(format!("document part read failed: {error}")))?;
        Ok(fnv1a(&bytes))
    }

    fn normalized_load(inputs: &BenchmarkInputs) -> Result<u64, AppError> {
        let session = inputs
            .engine
            .open_normalized_json(&inputs.normalized_json, OpenNormalizedOptions::default())
            .map_err(|error| AppError::new(format!("normalized load failed: {error}")))?;
        let snapshot = session
            .snapshot()
            .map_err(|error| AppError::new(format!("normalized snapshot failed: {error}")))?;
        Ok(u64::try_from(snapshot.body.len()).unwrap_or(u64::MAX))
    }

    fn typing_100_graphemes(inputs: &BenchmarkInputs) -> Result<u64, AppError> {
        let session = inputs
            .engine
            .create_blank()
            .map_err(|error| AppError::new(format!("blank session creation failed: {error}")))?;
        let snapshot = session
            .snapshot()
            .map_err(|error| AppError::new(format!("blank snapshot failed: {error}")))?;
        let paragraph = match &snapshot.body[0] {
            BlockSnapshot::Paragraph(paragraph) => paragraph.id.clone(),
        };
        let mut revision = snapshot.revision;
        for offset in 0..100 {
            let result = session
                .insert_text(InsertTextRequest {
                    base_revision: revision,
                    at: Position {
                        node: paragraph.clone(),
                        grapheme_offset: offset,
                        affinity: Affinity::After,
                    },
                    text: "a".to_owned(),
                    marks: BTreeSet::new(),
                })
                .map_err(|error| AppError::new(format!("typing transaction failed: {error}")))?;
            revision = result.revision;
        }
        let snapshot = session
            .snapshot()
            .map_err(|error| AppError::new(format!("typing snapshot failed: {error}")))?;
        let text_bytes = match &snapshot.body[0] {
            BlockSnapshot::Paragraph(paragraph) => paragraph
                .inlines
                .iter()
                .map(|inline| match inline {
                    InlineSnapshot::Text { text, .. } => text.len(),
                })
                .sum::<usize>(),
        };
        Ok((revision.get() << 32) | u64::try_from(text_bytes).unwrap_or(u64::MAX))
    }

    const fn fnv1a(bytes: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut index = 0;
        while index < bytes.len() {
            hash ^= bytes[index] as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            index += 1;
        }
        hash
    }

    fn normalized_document_json() -> Result<Vec<u8>, AppError> {
        let mut body = Vec::with_capacity(100);
        for index in 1_u128..=100 {
            body.push(serde_json::json!({
                "type": "paragraph",
                "id": format!("{:032x}", index + 1),
                "inlines": [{
                    "type": "text",
                    "text": format!("Paragraph {index}: deterministic OpenDoc benchmark text."),
                    "marks": []
                }]
            }));
        }
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 0,
            "documentId": format!("{:032x}", 1_u128),
            "body": body,
            "extensions": {}
        }))
        .map_err(|error| AppError::new(format!("benchmark JSON generation failed: {error}")))
    }

    fn median(sorted: &[u64]) -> Result<u64, AppError> {
        if sorted.is_empty() {
            return Err(AppError::new("cannot compute median of no samples"));
        }
        let middle = sorted.len() / 2;
        if sorted.len() % 2 == 1 {
            Ok(sorted[middle])
        } else {
            let sum = u128::from(sorted[middle - 1]) + u128::from(sorted[middle]);
            u64::try_from(sum / 2).map_err(|_| AppError::new("median overflowed"))
        }
    }

    fn percentile_nearest_rank(sorted: &[u64], percentile: u32) -> Result<u64, AppError> {
        if sorted.is_empty() || percentile == 0 || percentile > 100 {
            return Err(AppError::new("invalid percentile input"));
        }
        let count = u64::try_from(sorted.len()).map_err(|_| AppError::new("too many samples"))?;
        let rank = (count * u64::from(percentile)).div_ceil(100);
        let index = usize::try_from(rank - 1).map_err(|_| AppError::new("rank overflowed"))?;
        Ok(sorted[index])
    }

    fn rust_version() -> Result<String, AppError> {
        let output = Command::new("rustc")
            .arg("--version")
            .output()
            .map_err(|error| AppError::new(format!("rustc version query failed: {error}")))?;
        if !output.status.success() {
            return Err(AppError::new("rustc version query returned a failure"));
        }
        let value = String::from_utf8(output.stdout)
            .map_err(|_| AppError::new("rustc version output was not UTF-8"))?;
        let value = value.trim();
        if value.is_empty() || value.len() > 128 {
            return Err(AppError::new("rustc version output was invalid"));
        }
        Ok(value.to_owned())
    }

    fn write_report_atomically(path: &Path, report: &BenchmarkReport) -> Result<(), AppError> {
        if path.exists() {
            return Err(AppError::new(format!(
                "output already exists: {}",
                path.display()
            )));
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| {
            AppError::new(format!(
                "could not create output directory {}: {error}",
                parent.display()
            ))
        })?;
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::new("output file name must be valid UTF-8"))?;
        let temporary = parent.join(format!(
            ".{file_name}.tmp-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| AppError::new("system clock is before the Unix epoch"))?
                .as_nanos()
        ));
        let result = write_temporary_report(&temporary, path, report);
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn write_temporary_report(
        temporary: &Path,
        output: &Path,
        report: &BenchmarkReport,
    ) -> Result<(), AppError> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temporary)
            .map_err(|error| {
                AppError::new(format!(
                    "could not create temporary report {}: {error}",
                    temporary.display()
                ))
            })?;
        serde_json::to_writer_pretty(&mut file, report)
            .map_err(|error| AppError::new(format!("report serialization failed: {error}")))?;
        file.write_all(b"\n")
            .map_err(|error| AppError::new(format!("report write failed: {error}")))?;
        file.sync_all()
            .map_err(|error| AppError::new(format!("report sync failed: {error}")))?;
        fs::rename(temporary, output)
            .map_err(|error| AppError::new(format!("atomic report rename failed: {error}")))
    }

    fn read_report(path: &Path) -> Result<BenchmarkReport, AppError> {
        let bytes = fs::read(path).map_err(|error| {
            AppError::new(format!(
                "could not read baseline {}: {error}",
                path.display()
            ))
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|error| AppError::new(format!("baseline report is invalid: {error}")))
    }

    fn compare_reports(
        current: &BenchmarkReport,
        baseline: &BenchmarkReport,
        override_basis_points: Option<u32>,
    ) -> Result<(), AppError> {
        validate_comparable_metadata(current, baseline)?;
        let current_cases = case_map(&current.cases, "current")?;
        let baseline_cases = case_map(&baseline.cases, "baseline")?;
        if current_cases.keys().collect::<Vec<_>>() != baseline_cases.keys().collect::<Vec<_>>() {
            return Err(AppError::new(
                "current and baseline workload ID sets do not match",
            ));
        }

        for (id, current_case) in current_cases {
            let baseline_case = baseline_cases
                .get(id)
                .ok_or_else(|| AppError::new(format!("baseline workload {id} is missing")))?;
            if current_case.work_units != baseline_case.work_units
                || current_case.iterations_per_sample != baseline_case.iterations_per_sample
                || current_case.output_checksum != baseline_case.output_checksum
            {
                return Err(AppError::new(format!(
                    "workload {id} definition differs from its baseline"
                )));
            }
            let allowed_basis_points = override_basis_points
                .map(|value| value.min(baseline_case.max_regression_basis_points))
                .unwrap_or(baseline_case.max_regression_basis_points);
            let relative_allowance = u128::from(baseline_case.median_nanoseconds_per_iteration)
                * u128::from(allowed_basis_points)
                / 10_000;
            let allowance =
                relative_allowance.max(u128::from(baseline_case.absolute_noise_nanoseconds));
            let maximum = u128::from(baseline_case.median_nanoseconds_per_iteration) + allowance;
            if u128::from(current_case.median_nanoseconds_per_iteration) > maximum {
                return Err(AppError::new(format!(
                    "workload {id} regressed: current median {} ns, baseline {} ns, maximum {} ns",
                    current_case.median_nanoseconds_per_iteration,
                    baseline_case.median_nanoseconds_per_iteration,
                    maximum
                )));
            }
        }
        Ok(())
    }

    fn validate_comparable_metadata(
        current: &BenchmarkReport,
        baseline: &BenchmarkReport,
    ) -> Result<(), AppError> {
        if current.schema_version != REPORT_SCHEMA_VERSION
            || baseline.schema_version != REPORT_SCHEMA_VERSION
        {
            return Err(AppError::new("benchmark report schema is incompatible"));
        }
        if current.mode != RunMode::Full || baseline.mode != RunMode::Full {
            return Err(AppError::new("smoke reports cannot be used as baselines"));
        }
        if current.build_profile != "release" || baseline.build_profile != "release" {
            return Err(AppError::new(
                "baseline comparison requires release reports",
            ));
        }
        if current.environment.id != baseline.environment.id
            || current.environment.operating_system != baseline.environment.operating_system
            || current.environment.architecture != baseline.environment.architecture
        {
            return Err(AppError::new(
                "benchmark environment does not match the baseline",
            ));
        }
        Ok(())
    }

    fn case_map<'a>(
        cases: &'a [CaseReport],
        report_name: &str,
    ) -> Result<BTreeMap<&'a str, &'a CaseReport>, AppError> {
        let mut mapped = BTreeMap::new();
        for case in cases {
            if mapped.insert(case.id.as_str(), case).is_some() {
                return Err(AppError::new(format!(
                    "{report_name} report contains duplicate workload {}",
                    case.id
                )));
            }
        }
        Ok(mapped)
    }

    fn print_summary(report: &BenchmarkReport) {
        for case in &report.cases {
            println!(
                "{}: median={} ns p95={} ns",
                case.id, case.median_nanoseconds_per_iteration, case.p95_nanoseconds_per_iteration
            );
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn arguments(values: &[&str]) -> impl Iterator<Item = String> {
            values.iter().map(ToString::to_string)
        }

        fn case(id: &str, median: u64) -> CaseReport {
            CaseReport {
                id: id.to_owned(),
                work_units: 10,
                warmup_samples: 3,
                measured_samples: 15,
                iterations_per_sample: 5,
                output_checksum: "0123456789abcdef".to_owned(),
                minimum_nanoseconds_per_iteration: median,
                median_nanoseconds_per_iteration: median,
                p95_nanoseconds_per_iteration: median,
                max_regression_basis_points: 2_000,
                absolute_noise_nanoseconds: 50,
            }
        }

        fn report(id: &str, median: u64) -> BenchmarkReport {
            BenchmarkReport {
                schema_version: REPORT_SCHEMA_VERSION,
                runner_version: "0.0.1".to_owned(),
                source_revision: "revision".to_owned(),
                source_state: SourceState::Clean,
                generated_at_unix_seconds: 1,
                build_profile: "release".to_owned(),
                environment: Environment {
                    id: "controlled-linux".to_owned(),
                    operating_system: "linux".to_owned(),
                    architecture: "x86_64".to_owned(),
                    rust_version: "rustc test".to_owned(),
                    logical_cpus: 4,
                },
                mode: RunMode::Full,
                cases: vec![case(id, median)],
            }
        }

        #[test]
        fn parses_smoke_defaults_and_rejects_ambiguous_options() {
            let ParseOutcome::Run(config) =
                Config::parse(arguments(&["--smoke", "--output", "target/smoke.json"])).unwrap()
            else {
                panic!("expected runnable config");
            };
            assert_eq!(config.mode, RunMode::Smoke);
            assert_eq!(config.environment_id, "ci-smoke");
            assert_eq!(config.source_state, SourceState::Unknown);

            assert!(
                Config::parse(arguments(&[
                    "--smoke", "--output", "a.json", "--output", "b.json"
                ]))
                .is_err()
            );
            assert!(Config::parse(arguments(&["--smoke", "--output", "../outside.json"])).is_err());
            assert!(
                Config::parse(arguments(&[
                    "--output",
                    "full.json",
                    "--environment-id",
                    "runner"
                ]))
                .is_err()
            );
            assert!(
                Config::parse(arguments(&[
                    "--output",
                    "full.json",
                    "--environment-id",
                    "runner",
                    "--source-revision",
                    "revision",
                    "--source-state",
                    "unknown"
                ]))
                .is_err()
            );
        }

        #[test]
        fn computes_median_and_nearest_rank_percentile() {
            assert_eq!(median(&[10, 20, 30]).unwrap(), 20);
            assert_eq!(median(&[10, 20, 30, 40]).unwrap(), 25);
            let samples = (1_u64..=20).collect::<Vec<_>>();
            assert_eq!(percentile_nearest_rank(&samples, 95).unwrap(), 19);
            assert!(median(&[]).is_err());
            assert!(percentile_nearest_rank(&samples, 0).is_err());
        }

        #[test]
        fn regression_check_uses_larger_relative_or_absolute_allowance() {
            let baseline = report("case", 1_000);
            assert!(compare_reports(&report("case", 1_200), &baseline, None).is_ok());
            assert!(compare_reports(&report("case", 1_201), &baseline, None).is_err());

            let baseline = report("case", 100);
            assert!(compare_reports(&report("case", 150), &baseline, None).is_ok());
            assert!(compare_reports(&report("case", 151), &baseline, None).is_err());
        }

        #[test]
        fn regression_override_can_only_tighten_and_metadata_must_match() {
            let baseline = report("case", 1_000);
            assert!(compare_reports(&report("case", 1_150), &baseline, Some(1_000)).is_err());
            assert!(compare_reports(&report("case", 1_200), &baseline, Some(5_000)).is_ok());

            let mut incompatible = report("case", 1_000);
            incompatible.environment.id = "other-runner".to_owned();
            assert!(compare_reports(&incompatible, &baseline, None).is_err());

            let missing = report("other-case", 1_000);
            assert!(compare_reports(&missing, &baseline, None).is_err());
        }

        #[test]
        fn report_schema_rejects_unknown_fields() {
            let value = serde_json::to_value(report("case", 1_000)).unwrap();
            let mut object = value.as_object().unwrap().clone();
            object.insert("unexpected".to_owned(), serde_json::Value::Bool(true));
            assert!(
                serde_json::from_value::<BenchmarkReport>(serde_json::Value::Object(object))
                    .is_err()
            );
        }

        #[test]
        fn workload_inputs_and_expected_checksums_are_valid() {
            let inputs = BenchmarkInputs {
                normalized_json: normalized_document_json().unwrap(),
                engine: Engine::new(EngineConfig {
                    id_namespace: 0x4f50_454e_444f_4300,
                })
                .unwrap(),
            };
            for definition in benchmark_definitions() {
                assert_eq!(
                    (definition.execute)(&inputs).unwrap(),
                    definition.expected_unit_checksum,
                    "{}",
                    definition.id
                );
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    native::main();
}
