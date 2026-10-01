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
use casual_doc_transaction::session::ServerSession;
use serde::{Deserialize, Serialize};

/// One durable record.
///
/// Two variants and not three: there is no "refused" record, because a refusal changed nothing
/// and a log of non-events is a log nobody can replay cheaply.
#[derive(Clone, Debug, Deserialize, Serialize)]
enum Record {
    /// The relay's whole state at a point in the order. Written by [`Journal::compact`] and by
    /// [`Journal::create`].
    ///
    /// Boxed because it is much larger than the other variant, and an enum is as big as its
    /// widest arm — so without this every appended chunk would carry the checkpoint's footprint.
    Checkpoint(Box<ServerSession>),
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
        file.write_all(&encode_frame(&Record::Checkpoint(Box::new(
            session.clone(),
        ))))?;
        file.sync_data()?;
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

        let mut session = None;
        let mut pending: Vec<(Submission, Revision)> = Vec::new();
        for record in records {
            match record {
                Record::Checkpoint(state) => {
                    // A later checkpoint supersedes everything before it, including chunks
                    // logged before it — they are already inside it.
                    session = Some(*state);
                    pending.clear();
                }
                Record::Ordered {
                    submission,
                    revision,
                } => pending.push((submission, revision)),
            }
        }
        let mut session = session.ok_or(JournalError::NoCheckpoint)?;

        let replayed = pending.len();
        for (submission, logged) in pending {
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
                since_checkpoint: replayed,
            },
            Recovered {
                session,
                replayed,
                discarded_tail_bytes,
            },
        ))
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
            fresh.write_all(&encode_frame(&Record::Checkpoint(Box::new(
                session.clone(),
            ))))?;
            fresh.sync_data()?;
        }
        std::fs::rename(&staging, &self.path)?;
        self.file = OpenOptions::new().append(true).open(&self.path)?;
        self.since_checkpoint = 0;
        Ok(())
    }

    /// Every complete record in `bytes`, plus the size of a partial final frame.
    fn scan(bytes: &[u8]) -> Result<(Vec<Record>, usize), JournalError> {
        let mut records = Vec::new();
        let mut at = 0_usize;
        while at < bytes.len() {
            let rest = &bytes[at..];
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
                Ok(record) => records.push(record),
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

#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;
