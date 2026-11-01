mod checks;
mod job_board;

pub use checks::{CheckKind, CheckRow, CheckState, JobChecks, failed_gate, fit_band};
pub use job_board::{JobBoard, JobBoardService, JobView, LaneView};

pub use crate::discovery::domain::{CommuteTier, FitLevel, FreelanceGates, FreelanceTerms, Gate, GateReading, Lane, Signal, Verdict};
