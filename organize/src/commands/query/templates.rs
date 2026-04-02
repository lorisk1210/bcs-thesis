use refinery_protocol::QueryTemplate;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    Integer,
    IntegerList,
    String,
    StringList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryParamSpec {
    pub key: &'static str,
    pub prompt: &'static str,
    pub kind: ParamKind,
    pub optional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryTemplateSpec {
    pub template: QueryTemplate,
    pub params: &'static [QueryParamSpec],
}

const COHORT_FEASIBILITY_PARAMS: &[QueryParamSpec] = &[
    QueryParamSpec {
        key: "min_age",
        prompt: "Minimum age",
        kind: ParamKind::Integer,
        optional: true,
    },
    QueryParamSpec {
        key: "max_age",
        prompt: "Maximum age",
        kind: ParamKind::Integer,
        optional: true,
    },
    QueryParamSpec {
        key: "gender",
        prompt: "Gender",
        kind: ParamKind::String,
        optional: true,
    },
    QueryParamSpec {
        key: "condition_codes",
        prompt: "Condition codes (comma-separated)",
        kind: ParamKind::StringList,
        optional: true,
    },
    QueryParamSpec {
        key: "medication_codes",
        prompt: "Medication codes (comma-separated)",
        kind: ParamKind::StringList,
        optional: true,
    },
];

const COMPARATIVE_EFFECTIVENESS_PARAMS: &[QueryParamSpec] = &[
    QueryParamSpec {
        key: "min_age",
        prompt: "Minimum age",
        kind: ParamKind::Integer,
        optional: true,
    },
    QueryParamSpec {
        key: "max_age",
        prompt: "Maximum age",
        kind: ParamKind::Integer,
        optional: true,
    },
    QueryParamSpec {
        key: "gender",
        prompt: "Gender",
        kind: ParamKind::String,
        optional: true,
    },
    QueryParamSpec {
        key: "condition_codes",
        prompt: "Condition codes (comma-separated)",
        kind: ParamKind::StringList,
        optional: true,
    },
    QueryParamSpec {
        key: "exposed_medication_code",
        prompt: "Exposed medication code",
        kind: ParamKind::String,
        optional: false,
    },
    QueryParamSpec {
        key: "control_medication_code",
        prompt: "Control medication code",
        kind: ParamKind::String,
        optional: false,
    },
    QueryParamSpec {
        key: "outcome_observation_code",
        prompt: "Outcome observation code",
        kind: ParamKind::String,
        optional: false,
    },
];

const TIME_TO_EVENT_PARAMS: &[QueryParamSpec] = &[
    QueryParamSpec {
        key: "index_medication_code",
        prompt: "Index medication code",
        kind: ParamKind::String,
        optional: false,
    },
    QueryParamSpec {
        key: "event_condition_code",
        prompt: "Event condition code",
        kind: ParamKind::String,
        optional: false,
    },
    QueryParamSpec {
        key: "max_days",
        prompt: "Maximum days",
        kind: ParamKind::Integer,
        optional: true,
    },
    QueryParamSpec {
        key: "min_age",
        prompt: "Minimum age",
        kind: ParamKind::Integer,
        optional: true,
    },
    QueryParamSpec {
        key: "max_age",
        prompt: "Maximum age",
        kind: ParamKind::Integer,
        optional: true,
    },
    QueryParamSpec {
        key: "gender",
        prompt: "Gender",
        kind: ParamKind::String,
        optional: true,
    },
    QueryParamSpec {
        key: "condition_codes",
        prompt: "Condition codes (comma-separated)",
        kind: ParamKind::StringList,
        optional: true,
    },
];

const SUBGROUP_EFFECT_PARAMS: &[QueryParamSpec] = &[
    QueryParamSpec {
        key: "medication_code",
        prompt: "Medication code",
        kind: ParamKind::String,
        optional: false,
    },
    QueryParamSpec {
        key: "outcome_observation_code",
        prompt: "Outcome observation code",
        kind: ParamKind::String,
        optional: false,
    },
    QueryParamSpec {
        key: "subgroup",
        prompt: "Subgroup",
        kind: ParamKind::String,
        optional: true,
    },
    QueryParamSpec {
        key: "age_cutoffs",
        prompt: "Age cutoffs (comma-separated)",
        kind: ParamKind::IntegerList,
        optional: true,
    },
    QueryParamSpec {
        key: "min_age",
        prompt: "Minimum age",
        kind: ParamKind::Integer,
        optional: true,
    },
    QueryParamSpec {
        key: "max_age",
        prompt: "Maximum age",
        kind: ParamKind::Integer,
        optional: true,
    },
    QueryParamSpec {
        key: "gender",
        prompt: "Gender",
        kind: ParamKind::String,
        optional: true,
    },
    QueryParamSpec {
        key: "condition_codes",
        prompt: "Condition codes (comma-separated)",
        kind: ParamKind::StringList,
        optional: true,
    },
];

const DOSE_RESPONSE_PARAMS: &[QueryParamSpec] = &[
    QueryParamSpec {
        key: "medication_code",
        prompt: "Medication code",
        kind: ParamKind::String,
        optional: false,
    },
    QueryParamSpec {
        key: "outcome_observation_code",
        prompt: "Outcome observation code",
        kind: ParamKind::String,
        optional: false,
    },
];

const AE_SIGNAL_PARAMS: &[QueryParamSpec] = &[
    QueryParamSpec {
        key: "exposed_medication_code",
        prompt: "Exposed medication code",
        kind: ParamKind::String,
        optional: false,
    },
    QueryParamSpec {
        key: "control_medication_code",
        prompt: "Control medication code",
        kind: ParamKind::String,
        optional: false,
    },
    QueryParamSpec {
        key: "ae_condition_code",
        prompt: "Adverse event condition code",
        kind: ParamKind::String,
        optional: false,
    },
];

const DDI_SIGNAL_PARAMS: &[QueryParamSpec] = &[
    QueryParamSpec {
        key: "medication_a_code",
        prompt: "Medication A code",
        kind: ParamKind::String,
        optional: false,
    },
    QueryParamSpec {
        key: "medication_b_code",
        prompt: "Medication B code",
        kind: ParamKind::String,
        optional: false,
    },
    QueryParamSpec {
        key: "ae_condition_code",
        prompt: "Adverse event condition code",
        kind: ParamKind::String,
        optional: false,
    },
];

const TEMPLATE_SPECS: &[QueryTemplateSpec] = &[
    QueryTemplateSpec {
        template: QueryTemplate::CohortFeasibilityCount,
        params: COHORT_FEASIBILITY_PARAMS,
    },
    QueryTemplateSpec {
        template: QueryTemplate::ComparativeEffectivenessDelta,
        params: COMPARATIVE_EFFECTIVENESS_PARAMS,
    },
    QueryTemplateSpec {
        template: QueryTemplate::TimeToEventProxy,
        params: TIME_TO_EVENT_PARAMS,
    },
    QueryTemplateSpec {
        template: QueryTemplate::SubgroupEffectEstimate,
        params: SUBGROUP_EFFECT_PARAMS,
    },
    QueryTemplateSpec {
        template: QueryTemplate::DoseResponseTrend,
        params: DOSE_RESPONSE_PARAMS,
    },
    QueryTemplateSpec {
        template: QueryTemplate::AeIncidenceSignalProxy,
        params: AE_SIGNAL_PARAMS,
    },
    QueryTemplateSpec {
        template: QueryTemplate::DdiSignalProxy,
        params: DDI_SIGNAL_PARAMS,
    },
];

pub fn list_template_specs() -> &'static [QueryTemplateSpec] {
    TEMPLATE_SPECS
}

pub fn spec_for(template: QueryTemplate) -> &'static QueryTemplateSpec {
    TEMPLATE_SPECS
        .iter()
        .find(|spec| spec.template == template)
        .expect("all supported templates must have a spec")
}
