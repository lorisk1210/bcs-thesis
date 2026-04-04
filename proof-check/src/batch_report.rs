use crate::SectionStatus;
use crate::batch_models::{BatchReport, UtilityVerdictStatus};

pub fn batch_exit_code(report: &BatchReport) -> i32 {
    let any_compare_failure = report.queries.iter().any(|query| {
        let sections = [
            &query.compare_report.validation.smpc_parity,
            &query.compare_report.validation.coarsening_distortion,
            &query.compare_report.validation.final_release_utility,
        ];
        sections.iter().any(|section| {
            matches!(
                section.status,
                SectionStatus::Mismatch | SectionStatus::UnexpectedDistortion
            )
        })
    });

    if any_compare_failure
        || report
            .queries
            .iter()
            .any(|query| query.utility_verdict.status == UtilityVerdictStatus::NotPreserved)
    {
        return 1;
    }

    let any_compare_inconclusive = report.queries.iter().any(|query| {
        let sections = [
            &query.compare_report.validation.smpc_parity,
            &query.compare_report.validation.coarsening_distortion,
            &query.compare_report.validation.final_release_utility,
        ];
        sections
            .iter()
            .any(|section| section.status == SectionStatus::Inconclusive)
    });

    if any_compare_inconclusive
        || report.queries.iter().any(|query| {
            matches!(
                query.utility_verdict.status,
                UtilityVerdictStatus::Borderline
                    | UtilityVerdictStatus::Suppressed
                    | UtilityVerdictStatus::Inconclusive
            )
        })
    {
        2
    } else {
        0
    }
}
