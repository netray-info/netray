pub mod engine;
pub mod profile;

pub use engine::{
    CheckResult, CheckVerdict, OverallScore, SectionInput, SectionScore, compute_score,
};
pub use profile::ScoringProfile;
