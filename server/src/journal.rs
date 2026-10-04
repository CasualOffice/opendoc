// SPDX-License-Identifier: Apache-2.0

//! Durability for a relay — `107` 6.1: the ordered log, the snapshot, and compaction.
//!
//! # The established pattern, named before any code
//!
//! **A checkpoint plus a write-ahead tail, replayed on recovery.** Nothing here is new: it is
//! what every database and every log-structured store does, and the names are "checkpoint",
//! "WAL" and "truncation". The reason to say so first is that the tempting alternative — write
//! the whole state on every change — is what makes a relay's cost O(history) per edit, and the
//! reason the pattern exists is to make it O(1).
//!
//! # What is durable, and why it is this little
//!
//! ADR-047 decided a **dumb relay**: it orders chunks, holds no document, and runs no
//! transform. So the only state it can lose is *the order it imposed* — and the dedupe table
//! that stops a retried chunk being ordered twice. That is the whole of it. A relay that
//! persisted a document would be a relay that could disagree with its clients about one, which
//! is the failure mode ADR-047 exists to rule out.
//!
//! # Recovery verifies rather than trusts
//!
//! A record carries **the submission and the revision the relay ordered it at**, and recovery
//! *replays the decision*: it hands the submission back to [`ServerSession::commit`] and
//! checks the answer is the revision the record names. A disagreement is loud
//! ([`JournalError::DecisionDiffers`]) rather than a silently different order.
//!
//! That is deliberately stronger than replaying the *outcome*. Writing the ordered entry
//! straight into the history would make any recovery "succeed", including one where the
//! ordering rule had changed under the file — and a relay whose order is not the order its
//! clients were acknowledged against has diverged everybody without telling anyone. It is the
//! same argument ADR-051 makes for a snapshot: *verifiable, not trusted*.
//!
//! # A torn tail is expected; a torn middle is not
//!
//! A process can die between `write` and the next `write`, so the **last** record in a file may
//! be a partial frame. That is normal and is discarded with a count
//! ([`Recovered::discarded_tail_bytes`]). A frame that fails to decode anywhere *else* is
//! corruption and is refused: the records after it belong to an order this file can no longer
//! describe, and guessing is how a relay silently drops acknowledged work.
//!
//! # Complexity
//!
//! Appending is O(the chunk). Recovery is O(records since the last checkpoint). Compaction is
//! O(the retained history) and is the operation that keeps recovery from growing without bound
//! — which is why [`Journal::should_compact`] exists rather than a caller guessing.

use std::fs::{File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use casual_doc_transaction::codec::{CodecError, decode_frame, encode_frame, frame_len};
use casual_doc_transaction::protocol::{Outcome, Revision, Submission};
use casual_doc_transaction::session::{Admission, Ordered, ServerSession, SessionState};
use serde::{Deserialize, Serialize};

/// One durable record.
///
/// **Every variant is bounded by one chunk's worth of bytes**, which is the property that lets
/// this journal read with the codec's own frame bound and no special case of its own. The first
/// version of this file had a `Checkpoint(Box<ServerSession>)` that *embedded the retained
/// history* — up to `DEFAULT_RETAINED_REVISIONS × CHUNK_BUDGET_BYTES`, about 1.2 GB against a
/// 30 MB wire bound — and carried a derived `MAX_JOURNAL_FRAME_BYTES` to be able to read back
/// what it wrote. Splitting the history into one [`Record::Retained`] per entry removed the
/// special bound rather than enlarging it (ADR-058, amended).
///
/// **There is still no "refused" record**, because a refusal changed nothing and a log of
/// non-events is a log nobody can replay cheaply.
#[derive(Clone, Debug, Deserialize, Serialize)]
enum Record {
    /// The relay's state **without** its retained history, which the following
    /// [`Record::Retained`] frames carry. Written by [`Journal::compact`] and by
    /// [`Journal::create`].
    ///
    /// Still boxed: it is the widest arm, and an enum is as big as its widest, so without this
    /// every appended chunk would carry the participant tables' footprint.
    Checkpoint(Box<SessionState>),
    /// One retained history entry, belonging to the checkpoint that immediately precedes it.
    ///
    /// Only ever written straight after a `Checkpoint`, and a `Retained` anywhere else is
    /// [`JournalError::Corrupt`] — an orphan entry has no state to attach to, and guessing which
    /// checkpoint it belonged to is how a relay rebuilds the wrong retained window.
    Retained(Ordered),
    /// A participant was admitted, and on what terms.
    ///
    /// Written **before the join is answered**, for the same reason an ordered chunk is written
    /// before its acknowledgement: a client that has been handed a participant number must not
    /// be able to outlive the record of it. Before this record existed, a crash rolled
    /// `next_client` back to the last checkpoint while the dedupe table was rebuilt by replay,
    /// so a reconnecting participant could be handed a number that already had a dedupe entry
    /// — measured, see [`Journal::append_admission`].
    Admitted(Admission),
    /// A chunk that was ordered, and **the decision that was taken about it**.
    Ordered {
        /// What the client offered, verbatim.
        submission: Submission,
        /// The revision the relay ordered it at. Recovery checks its replay against this.
        revision: Revision,
    },
}

/// Why a journal could not be read or written.
#[derive(Debug)]
#[non_exhaustive]
pub enum JournalError {
    /// The underlying file could not be read, written or flushed.
    Io(std::io::Error),
    /// A frame failed to decode somewhere other than at the very end of the file.
    Corrupt {
        /// How many bytes into the file the bad frame began.
        at: u64,
        /// What the codec said.
        cause: CodecError,
    },
    /// A `Record::Retained` frame appeared somewhere other than directly after a checkpoint.
    ///
    /// Its own variant rather than a reused [`JournalError::Corrupt`], because the frame decoded
    /// perfectly — what is damaged is the file's *structure*, and telling an operator "a corrupt
    /// frame" about bytes that parsed would send them looking at the wrong thing.
    OrphanEntry {
        /// How many bytes into the file the misplaced frame began.
        at: u64,
    },
    /// The file holds no checkpoint, so there is no state to replay onto.
    ///
    /// Distinct from an empty path: a missing file is a room that does not exist yet and is
    /// [`Journal::create`]'s business. A *present* file with no checkpoint is damaged.
    NoCheckpoint,
    /// Replaying a logged submission produced a different revision from the one logged.
    ///
    /// The loud failure the whole design is for. It means the ordering rule the file was
    /// written under and the one this build holds are not the same rule.
    DecisionDiffers {
        /// What the file says the relay decided.
        logged: Revision,
        /// What this build decides now.
        replayed: Option<Revision>,
    },
}

impl core::fmt::Display for JournalError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "journal i/o: {error}"),
            Self::Corrupt { at, cause } => {
                write!(
                    formatter,
                    "a corrupt frame at byte {at} of the journal: {cause}"
                )
            }
            Self::OrphanEntry { at } => write!(
                formatter,
                "a retained-history entry at byte {at} of the journal follows no checkpoint"
            ),
            Self::NoCheckpoint => write!(formatter, "the journal holds no checkpoint"),
            Self::DecisionDiffers { logged, replayed } => write!(
                formatter,
                "replay ordered a logged chunk at {replayed:?}, but the journal says {}",
                logged.get()
            ),
        }
    }
}

impl core::error::Error for JournalError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for JournalError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// What a recovery found, so a caller can report it rather than infer it.
#[derive(Debug)]
pub struct Recovered {
    /// The relay state, replayed.
    pub session: ServerSession,
    /// How many logged chunks were replayed onto the checkpoint.
    pub replayed: usize,
    /// How many logged admissions were replayed onto the checkpoint.
    ///
    /// Reported separately from `replayed` because the two answer different operator questions:
    /// one says how much ordering work the restart had to redo, the other how many participants
    /// had joined since the last checkpoint and therefore kept their participant numbers.
    pub readmitted: usize,
    /// How many bytes of a partial final frame were discarded — a crash between appends.
    ///
    /// Non-zero is **normal**, and worth reporting rather than hiding: it is the only evidence
    /// that the previous process did not shut down cleanly.
    pub discarded_tail_bytes: usize,
}

/// A relay's durable ordered log.
#[derive(Debug)]
pub struct Journal {
    path: PathBuf,
    file: File,
    /// Records appended since the last checkpoint — what [`Journal::should_compact`] reads.
    since_checkpoint: usize,
}

/// How many appended chunks are allowed to accumulate before a checkpoint is worth writing.
///
/// A bound on **recovery time**, not on file size, which is why it is a record count: recovery
/// replays this many `commit` calls, and each is O(1). Compaction itself is O(retained
/// history), so a smaller number makes recovery faster and steady-state writing slower. 1024 is
/// one order of magnitude above the relay's own retained-history bound, so a compaction never
/// happens more often than the history it rewrites turns over.
pub const CHECKPOINT_EVERY: usize = 1024;

/// This journal reads with the codec's own
/// [`MAX_FRAME_BYTES`](casual_doc_transaction::codec::MAX_FRAME_BYTES) and has no bound of its
/// own — which is the point of `Record`'s shape.
///
/// It used to need one. `MAX_JOURNAL_FRAME_BYTES` was
/// `DEFAULT_RETAINED_REVISIONS * CHUNK_BUDGET_BYTES + 16 MB` — about 1.2 GB — because a
/// checkpoint embedded the relay's whole retained history, and reading the journal with the
/// socket's 30 MB bound would have meant a busy relay writing a checkpoint it could never read
/// back: `compact` would succeed and the next `open` would refuse its own file. The special
/// bound was the right *stopgap* and the wrong *shape*, and its own doc comment said so.
///
/// Writing the history as one bounded frame per entry — which every other record here already
/// was — makes every frame chunk-sized, so the special case is **deleted rather than
/// documented**. A local file is still a different threat model from a socket; the difference no
/// longer needs expressing as a larger number.
impl Journal {
    /// Creates a journal for a **new** room, writing the opening checkpoint.
    ///
    /// Refuses if the file already exists: creating over a room's order would destroy the only
    /// copy of it. `152` records that the **host** creates a room rather than the first client,
    /// and this is where that is enforced in the file system.
    ///
    /// # Errors
    ///
    /// [`JournalError::Io`], including [`std::io::ErrorKind::AlreadyExists`].
    pub fn create(path: impl AsRef<Path>, session: &ServerSession) -> Result<Self, JournalError> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        write_checkpoint(&mut file, session)?;
        Ok(Self {
            path,
            file,
            since_checkpoint: 0,
        })
    }

    /// Opens an existing journal and replays it.
    ///
    /// # Errors
    ///
    /// [`JournalError`] — and a corrupt or disagreeing journal is an error rather than an empty
    /// room, because starting empty would silently drop every chunk the relay had already
    /// acknowledged.
    pub fn open(path: impl AsRef<Path>) -> Result<(Self, Recovered), JournalError> {
        let path = path.as_ref().to_path_buf();
        let mut bytes = Vec::new();
        File::open(&path)?.read_to_end(&mut bytes)?;
        let (records, discarded_tail_bytes) = Self::scan(&bytes)?;

        // Rebuilt in file order. `Retained` frames belong to the checkpoint they follow, so a
        // checkpoint discards both the entries and the pending records before it: they are
        // already inside it.
        let mut state: Option<SessionState> = None;
        let mut retained: Vec<Ordered> = Vec::new();
        let mut pending: Vec<Replay> = Vec::new();
        let mut at = 0_u64;
        for (record, offset) in records {
            at = offset;
            match record {
                Record::Checkpoint(checkpointed) => {
                    state = Some(*checkpointed);
                    retained.clear();
                    pending.clear();
                }
                // An orphan entry has no state to attach to, and attaching it to an earlier
                // checkpoint would rebuild the wrong retained window — so it is corruption
                // rather than something to place as best as possible.
                Record::Retained(_) if state.is_none() || !pending.is_empty() => {
                    return Err(JournalError::OrphanEntry { at });
                }
                Record::Retained(entry) => retained.push(entry),
                Record::Admitted(admission) => pending.push(Replay::Admitted(admission)),
                Record::Ordered {
                    submission,
                    revision,
                } => pending.push(Replay::Ordered {
                    submission,
                    revision,
                }),
            }
        }
        let state = state.ok_or(JournalError::NoCheckpoint)?;
        let mut session = ServerSession::restored(state, retained);

        let total = pending.len();
        let mut readmitted = 0_usize;
        for record in pending {
            let (submission, logged) = match record {
                // Replayed in file order and **ahead of every chunk that depends on it**, which
                // is what makes `commit`'s membership check survive a restart. A join is
                // journalled before it is answered, so an admission always precedes that
                // participant's first chunk in the file.
                Replay::Admitted(admission) => {
                    session.readmit(&admission);
                    readmitted += 1;
                    continue;
                }
                Replay::Ordered {
                    submission,
                    revision,
                } => (submission, revision),
            };
            let _ = at;
            // The verification, not a convenience: the decision is re-taken and compared.
            match session.commit(&submission) {
                Outcome::Ordered { revision } if revision == logged => {}
                Outcome::Ordered { revision } => {
                    return Err(JournalError::DecisionDiffers {
                        logged,
                        replayed: Some(revision),
                    });
                }
                // A duplicate on replay means the chunk is already inside the checkpoint at the
                // logged revision, which is consistent; anything else is not.
                Outcome::Duplicate { revision } if revision == logged => {}
                Outcome::Duplicate { revision } => {
                    return Err(JournalError::DecisionDiffers {
                        logged,
                        replayed: Some(revision),
                    });
                }
                Outcome::Refused { .. } => {
                    return Err(JournalError::DecisionDiffers {
                        logged,
                        replayed: None,
                    });
                }
            }
        }

        let file = OpenOptions::new().append(true).open(&path)?;
        Ok((
            Self {
                path,
                file,
                since_checkpoint: total,
            },
            Recovered {
                session,
                replayed: total - readmitted,
                readmitted,
                discarded_tail_bytes,
            },
        ))
    }

    /// Records that `admission` was granted, durably, **before the join is answered**.
    ///
    /// The join half of the same contract [`Journal::append`] holds for a chunk, and it exists
    /// because the absence of it was a live defect on `main`. Measured, with
    /// `probe_what_a_crash_actually_does_to_an_admission` before it was a guard: two
    /// participants joined after a checkpoint, one chunk was ordered and journalled, the process
    /// died. On recovery the order was intact at `Revision(1)` — and
    ///
    /// - `has_assigned` answered **false** for both participants, so the relay's own boundary
    ///   check refused chunks from a participant the order had already acknowledged;
    /// - a resume presenting a **known** key answered `Welcome`, not `Resumed`, so the
    ///   unacknowledged work that `152` §5.5 exists to preserve was discarded **with no
    ///   `TooFarBehind` and no announcement at all** — silent loss, which is worse than the
    ///   announced loss the handover expected;
    /// - and participant number 0 was handed out again, so the next holder's first chunk at
    ///   `Seq(1)` came back `Duplicate { revision: Revision(1) }` and vanished. That is the
    ///   same harm ADR-060's forged-submission fix closed, reachable through a crash instead of
    ///   through a forgery — and because a participant number *is* an `IdSpace` (ADR-051), two
    ///   live replicas would also have been minting colliding `NodeId`s.
    ///
    /// # Errors
    ///
    /// [`JournalError::Io`]. A caller that cannot journal must **not** admit.
    pub fn append_admission(&mut self, admission: &Admission) -> Result<(), JournalError> {
        self.file
            .write_all(&encode_frame(&Record::Admitted(admission.clone())))?;
        self.file.sync_data()?;
        self.since_checkpoint = self.since_checkpoint.saturating_add(1);
        Ok(())
    }

    /// Records that `submission` was ordered at `revision`, durably.
    ///
    /// Called **after** the relay's own decision and **before** the acknowledgement is sent: a
    /// client that has been told its chunk is ordered must not be able to outlive the record of
    /// it. That ordering is the whole contract of a write-ahead log and the reason
    /// [`sync_data`](File::sync_data) is not optional here.
    ///
    /// # Errors
    ///
    /// [`JournalError::Io`]. A caller that cannot journal must **not** acknowledge.
    pub fn append(
        &mut self,
        submission: &Submission,
        revision: Revision,
    ) -> Result<(), JournalError> {
        self.file.write_all(&encode_frame(&Record::Ordered {
            submission: submission.clone(),
            revision,
        }))?;
        self.file.sync_data()?;
        self.since_checkpoint = self.since_checkpoint.saturating_add(1);
        Ok(())
    }

    /// Whether enough chunks have accumulated that a checkpoint is worth writing.
    #[must_use]
    pub const fn should_compact(&self) -> bool {
        self.since_checkpoint >= CHECKPOINT_EVERY
    }

    /// Rewrites the journal as one checkpoint of `session`, discarding the replayed tail.
    ///
    /// Written to a sibling path and **renamed over** the original, so a crash mid-compaction
    /// leaves the old journal intact rather than a half-written new one. A rename within one
    /// directory is the atomic primitive every file format uses for this, and doing it any other
    /// way is how a compaction becomes the thing that loses the data.
    ///
    /// # Errors
    ///
    /// [`JournalError::Io`]. On failure the existing journal is untouched.
    pub fn compact(&mut self, session: &ServerSession) -> Result<(), JournalError> {
        let mut staging = self.path.clone().into_os_string();
        staging.push(".compacting");
        let staging = PathBuf::from(staging);
        {
            let mut fresh = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&staging)?;
            write_checkpoint(&mut fresh, session)?;
        }
        std::fs::rename(&staging, &self.path)?;
        self.file = OpenOptions::new().append(true).open(&self.path)?;
        self.since_checkpoint = 0;
        Ok(())
    }

    /// Every complete record in `bytes` with the offset it began at, plus the size of a partial
    /// final frame.
    ///
    /// The offsets are carried because an out-of-place `Retained` record is corruption and the
    /// error names where it was — "a corrupt frame at byte N" is the only form of that message
    /// an operator can act on.
    fn scan(bytes: &[u8]) -> Result<(Vec<(Record, u64)>, usize), JournalError> {
        let mut records = Vec::new();
        let mut at = 0_usize;
        while at < bytes.len() {
            let rest = &bytes[at..];
            // The codec's own bound, with no journal-specific widening: every `Record` variant
            // is now one chunk wide at most, which is what deleting `MAX_JOURNAL_FRAME_BYTES`
            // rests on.
            let length = match frame_len(rest) {
                Ok(length) => length,
                // The expected shape of a crash between appends: the file ends mid-frame.
                // Everything before it is intact and is kept.
                Err(CodecError::Truncated { .. } | CodecError::TooShort { .. }) => {
                    return Ok((records, rest.len()));
                }
                Err(cause) => {
                    return Err(JournalError::Corrupt {
                        at: at as u64,
                        cause,
                    });
                }
            };
            match decode_frame::<Record>(&rest[..length]) {
                Ok(record) => records.push((record, at as u64)),
                Err(cause) => {
                    return Err(JournalError::Corrupt {
                        at: at as u64,
                        cause,
                    });
                }
            }
            at += length;
        }
        Ok((records, 0))
    }
}

/// What one pending record asks recovery to do, in file order.
///
/// A plain enum rather than two vectors, because the **order between** an admission and a chunk
/// is the whole point: `commit` refuses a chunk from a participant it has not readmitted, so
/// sorting the two kinds apart would make recovery depend on an ordering the file already has.
enum Replay {
    Admitted(Admission),
    Ordered {
        submission: Submission,
        revision: Revision,
    },
}

/// Writes one checkpoint: the state as one frame, then **one bounded frame per retained entry**.
///
/// The ordering is the format: [`Journal::open`] attaches `Retained` frames to the checkpoint
/// they follow, so the state must be written first and nothing may be interleaved. Both writes
/// are followed by a single `sync_data`, because a checkpoint is only useful whole — a partially
/// synced one is a torn tail, which recovery already discards.
fn write_checkpoint(file: &mut File, session: &ServerSession) -> Result<(), JournalError> {
    let (state, retained) = session.checkpoint();
    file.write_all(&encode_frame(&Record::Checkpoint(Box::new(state))))?;
    for entry in retained {
        file.write_all(&encode_frame(&Record::Retained(entry.clone())))?;
    }
    file.sync_data()?;
    Ok(())
}

#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;
