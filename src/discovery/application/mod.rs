mod checks;
mod job_board;
mod terms;

pub use checks::{AD_ALONE, CheckKind, CheckRow, FreelanceGate, JobChecks, failed_gate, fit_band};
pub use job_board::{JobBoard, JobBoardService, JobView, LaneView};
pub use terms::{CONTRACT_TERM, Commute, TermPart, YourTerms};

pub use crate::discovery::domain::{CommuteTier, FitLevel, FreelanceGates, FreelanceTerms, Gate, GateReading, Lane, Signal, Verdict};
pub use crate::shared_kernel::CheckState;
