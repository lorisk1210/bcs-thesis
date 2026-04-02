use crate::ComparisonReport;
use crate::SectionStatus;

pub fn exit_code(report: &ComparisonReport) -> i32 {
    let sections = [
        &report.smpc_parity,
        &report.coarsening_distortion,
        &report.final_release_utility,
    ];
    if sections.iter().any(|section| {
        matches!(
            section.status,
            SectionStatus::Mismatch | SectionStatus::UnexpectedDistortion
        )
    }) {
        1
    } else if sections
        .iter()
        .any(|section| section.status == SectionStatus::Inconclusive)
    {
        2
    } else {
        0
    }
}
