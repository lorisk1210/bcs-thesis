use crate::ComparisonReport;
use crate::SectionStatus;

pub fn exit_code(report: &ComparisonReport) -> i32 {
    let sections = [
        &report.validation.smpc_parity,
        &report.validation.coarsening_distortion,
        &report.validation.final_release_utility,
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
