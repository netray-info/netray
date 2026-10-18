// fixture, not compiled: a grouped import of `score` that rustfmt wrapped
use netray_engine::{
    SectionEvent, SectionOutcome, catalogue::CheckSpec, profile::Profile,
    score,
};

fn grade(run: &Run, p: &Profile) -> Grade {
    score(run, p)
}
