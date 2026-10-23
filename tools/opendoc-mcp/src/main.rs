// SPDX-License-Identifier: Apache-2.0

//! `opendoc-mcp`: a Model Context Protocol server over stdio for OpenDoc
//! documents (`docs/178`, ADR-072). Newline-delimited JSON-RPC 2.0 on stdin
//! and stdout; diagnostics and the audit log on stderr. Stateless in the MCP
//! 2026-07-28 sense: cross-call state is the document handle `open_document`
//! mints, passed as an ordinary argument.
//!
//! ```text
//! opendoc-mcp --root ~/contracts [--root DIR]... [--write-root DIR]
//!             [--profile read|comment|suggest|edit|review] [--author NAME]
//!             [--allow-overwrite]
//! ```
//!
//! The default profile is `read` and the default write root is none, so out of
//! the box the server edits nothing and writes nothing to disk. Direct
//! (untracked) edits need `--profile edit` or higher: the explicit grant.

#![forbid(unsafe_code)]

mod catalog;
mod paths;
mod server;

use std::io::{BufRead, Write};
use std::path::PathBuf;

use casual_doc_ai::Profile;
use serde_json::{Value, json};

use crate::paths::Roots;
use crate::server::{Options, Server};

const USAGE: &str = "usage: opendoc-mcp --root DIR [--root DIR]... [--write-root DIR]... \
[--profile read|comment|suggest|edit|review] [--author NAME] [--allow-overwrite]";

/// Most bytes in one request line: a 1 MiB plan plus its JSON overhead.
const MAX_LINE: usize = 4 * 1024 * 1024;

fn parse_args(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut read = Vec::new();
    let mut write = Vec::new();
    let mut profile = Profile::Read;
    let mut author = "AI assistant".to_owned();
    let mut allow_overwrite = false;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().ok_or_else(|| format!("{flag} needs a value"));
        match arg.as_str() {
            "--root" => read.push(PathBuf::from(value("--root")?)),
            "--write-root" => write.push(PathBuf::from(value("--write-root")?)),
            "--profile" => {
                let name = value("--profile")?;
                profile = Profile::parse(&name).ok_or_else(|| format!("unknown profile {name}"))?;
            }
            "--author" => {
                author = value("--author")?.trim().to_owned();
                if author.is_empty() || author.len() > 255 {
                    return Err("--author must be 1 to 255 bytes".to_owned());
                }
            }
            "--allow-overwrite" => allow_overwrite = true,
            "--help" | "-h" => return Err(USAGE.to_owned()),
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    if read.is_empty() && write.is_empty() {
        return Err(format!("at least one --root is required\n{USAGE}"));
    }
    Ok(Options {
        roots: Roots::new(&read, &write)?,
        profile,
        author,
        allow_overwrite,
    })
}

fn reply(out: &mut impl Write, id: &Value, result: Result<Value, (i64, String)>) {
    let message = match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    };
    let _ = writeln!(out, "{message}");
    let _ = out.flush();
}

fn main() {
    let options = match parse_args(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            let _ = writeln!(std::io::stderr(), "{message}");
            std::process::exit(2);
        }
    };
    let mut server = Server::new(options);
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    let mut line = String::new();
    let mut input = stdin.lock();
    loop {
        line.clear();
        match input.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if line.trim().is_empty() {
            continue;
        }
        if line.len() > MAX_LINE {
            reply(
                &mut out,
                &Value::Null,
                Err((-32600, "request too large".to_owned())),
            );
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            reply(
                &mut out,
                &Value::Null,
                Err((-32700, "parse error".to_owned())),
            );
            continue;
        };
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // A response from the client, or junk: nothing to answer.
            continue;
        };
        let Some(id) = message.get("id") else {
            // A notification (`notifications/initialized`, cancellation): no reply.
            continue;
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let result = server
            .handle(method, &params)
            .map_err(|error| (error.code, error.message));
        reply(&mut out, id, result);
    }
}
