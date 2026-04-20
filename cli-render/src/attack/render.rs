use std::fmt::Write;

use crate::OutputMode;
use crate::common::{key_value, section_header, status_badge, title};
use crate::frame::{
    BLUE, BOLD, CYAN, DARK_GRAY, DIM, GREEN, MAGENTA, RED, RESET, YELLOW, frame_cli_output,
};

use super::data::{AttackPlantCanaryData, AttackRunData, AttackSweepCellData, AttackSweepData};

// Per-axis ANSI colors for sweep-cell rendering. Keeping the color the same
// for a given axis across every cell line lets users visually lock onto a
// single dimension (e.g. "all the blue epsilons") without needing a new
// section per value. Green/yellow/red are reserved for status badges and
// group summaries, so we stick to blue/magenta/cyan here.
const EPSILON_COLOR: &str = BLUE;
const TARGET_COLOR: &str = MAGENTA;
const KNOWLEDGE_COLOR: &str = CYAN;
const BUDGET_COLOR: &str = DARK_GRAY;

fn axis_pair(label: &str, value: &str, color: &str) -> String {
    format!("{DIM}{label}={RESET}{BOLD}{color}{value}{RESET}")
}

pub fn render_attack_run_report(mode: OutputMode, data: &AttackRunData) -> String {
    let inner = match mode {
        OutputMode::Plain => render_attack_run_plain(data),
        OutputMode::Pretty => render_attack_run_pretty(mode, data),
    };
    frame_cli_output(mode, inner)
}

pub fn render_attack_sweep_report(mode: OutputMode, data: &AttackSweepData) -> String {
    let inner = match mode {
        OutputMode::Plain => render_attack_sweep_plain(data),
        OutputMode::Pretty => render_attack_sweep_pretty(mode, data),
    };
    frame_cli_output(mode, inner)
}

pub fn render_attack_plant_canary(mode: OutputMode, data: &AttackPlantCanaryData) -> String {
    let inner = match mode {
        OutputMode::Plain => {
            let mut out = String::new();
            let _ = writeln!(out, "node_id: {}", data.node_id);
            let _ = writeln!(out, "node_input_dir: {}", data.node_input_dir);
            let _ = writeln!(out, "bundle_path: {}", data.bundle_path);
            let _ = writeln!(out, "patient_id: {}", data.patient_id);
            let _ = writeln!(out, "condition_code: {}", data.condition_code);
            let _ = writeln!(out, "medication_code: {}", data.medication_code);
            let _ = writeln!(out, "gender: {}", data.gender);
            let _ = writeln!(out, "birth_date: {}", data.birth_date);
            out
        }
        OutputMode::Pretty => {
            let t = title(mode, "check-attack plant-canary");
            let mut out = format!("{t}\n\n");
            let _ = writeln!(out, "{}", key_value(mode, "node_id", &data.node_id));
            let _ = writeln!(
                out,
                "{}",
                key_value(mode, "node_input_dir", &data.node_input_dir)
            );
            let _ = writeln!(out, "{}", key_value(mode, "bundle_path", &data.bundle_path));
            let _ = writeln!(out, "{}", key_value(mode, "patient_id", &data.patient_id));
            let _ = writeln!(
                out,
                "{}",
                key_value(mode, "condition_code", &data.condition_code)
            );
            let _ = writeln!(
                out,
                "{}",
                key_value(mode, "medication_code", &data.medication_code)
            );
            let _ = writeln!(out, "{}", key_value(mode, "gender", &data.gender));
            let _ = writeln!(out, "{}", key_value(mode, "birth_date", &data.birth_date));
            out
        }
    };
    frame_cli_output(mode, inner)
}

fn render_attack_run_plain(data: &AttackRunData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "attack_kind: {}", data.attack_kind);
    let _ = writeln!(out, "evaluation_config: {}", data.evaluation_config);
    if let Some(eps) = data.epsilon {
        let _ = writeln!(out, "epsilon: {eps:.4}");
    }
    let _ = writeln!(out, "min_cohort: {}", data.min_cohort);
    let _ = writeln!(out, "coarsening_disabled: {}", data.disable_coarsening);
    let _ = writeln!(out, "target_type: {}", data.target_type);
    if let Some(id) = &data.target_id {
        let _ = writeln!(out, "target_id: {id}");
    }
    let _ = writeln!(out, "knowledge_level: {}", data.knowledge_level);
    let _ = writeln!(out, "query_budget: {}", data.query_budget);
    let _ = writeln!(out, "queries_used: {}", data.queries_used);
    let _ = writeln!(out, "suppressed_queries: {}", data.suppressed_queries);
    let _ = writeln!(out, "blocked_queries: {}", data.blocked_queries);
    let _ = writeln!(out, "outcome: {}", data.outcome);
    let _ = writeln!(out, "success: {}", if data.success { "yes" } else { "no" });
    if let Some(size) = data.initial_candidate_set_size {
        let _ = writeln!(out, "initial_candidate_set_size: {size}");
    }
    if let Some(size) = data.final_candidate_set_size {
        let _ = writeln!(out, "final_candidate_set_size: {size}");
    }
    if let Some(post) = data.final_posterior {
        let _ = writeln!(out, "final_posterior: {post:.4}");
    }
    if let Some(acc) = data.node_guess_accuracy {
        let _ = writeln!(out, "node_guess_accuracy: {acc:.4}");
    }
    if !data.notes.is_empty() {
        let _ = writeln!(out, "notes:");
        for note in &data.notes {
            let _ = writeln!(out, "  - {note}");
        }
    }
    out
}

fn render_attack_run_pretty(mode: OutputMode, data: &AttackRunData) -> String {
    let t = title(mode, "check-attack run");
    let status_name = if data.success {
        "failed" // a successful adversary = the defense failed
    } else {
        "passed"
    };
    let badge = status_badge(mode, status_name);
    let mut out = format!("{t}\n\n  {badge}\n\n");
    let _ = writeln!(out, "{}", key_value(mode, "attack_kind", &data.attack_kind));
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "evaluation_config", &data.evaluation_config)
    );
    if let Some(eps) = data.epsilon {
        let _ = writeln!(out, "{}", key_value(mode, "epsilon", &format!("{eps:.4}")));
    }
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "min_cohort", &data.min_cohort.to_string())
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(
            mode,
            "coarsening_disabled",
            &data.disable_coarsening.to_string()
        )
    );
    let _ = writeln!(out, "{}", key_value(mode, "target_type", &data.target_type));
    if let Some(id) = &data.target_id {
        let _ = writeln!(out, "{}", key_value(mode, "target_id", id));
    }
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "knowledge_level", &data.knowledge_level)
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "query_budget", &data.query_budget.to_string())
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "queries_used", &data.queries_used.to_string())
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(
            mode,
            "suppressed_queries",
            &data.suppressed_queries.to_string()
        )
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "blocked_queries", &data.blocked_queries.to_string())
    );
    let _ = writeln!(out, "{}", key_value(mode, "outcome", &data.outcome));
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "success", if data.success { "yes" } else { "no" })
    );
    if let Some(size) = data.initial_candidate_set_size {
        let _ = writeln!(
            out,
            "{}",
            key_value(mode, "initial_candidate_set_size", &size.to_string())
        );
    }
    if let Some(size) = data.final_candidate_set_size {
        let _ = writeln!(
            out,
            "{}",
            key_value(mode, "final_candidate_set_size", &size.to_string())
        );
    }
    if let Some(post) = data.final_posterior {
        let _ = writeln!(
            out,
            "{}",
            key_value(mode, "final_posterior", &format!("{post:.4}"))
        );
    }
    if let Some(acc) = data.node_guess_accuracy {
        let _ = writeln!(
            out,
            "{}",
            key_value(mode, "node_guess_accuracy", &format!("{acc:.4}"))
        );
    }
    if !data.notes.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", section_header(mode, "Notes"));
        for note in &data.notes {
            let _ = writeln!(out, "    {DARK_GRAY}•{RESET} {note}");
        }
    }
    out
}

fn render_attack_sweep_plain(data: &AttackSweepData) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "started_at: {}", data.started_at);
    let _ = writeln!(out, "min_cohort: {}", data.min_cohort);
    let _ = writeln!(out, "default_epsilon: {:.4}", data.default_epsilon);
    let _ = writeln!(out, "input_dir: {}", data.input_dir);
    let _ = writeln!(out, "as_of_date: {}", data.as_of_date);
    let _ = writeln!(out, "attacks: {}", data.attacks.join(","));
    let _ = writeln!(out, "configs: {}", data.configs.join(","));
    let eps_line = data
        .epsilons
        .iter()
        .map(|e| format!("{e:.3}"))
        .collect::<Vec<_>>()
        .join(",");
    let _ = writeln!(out, "epsilons: {eps_line}");
    let _ = writeln!(out, "target_types: {}", data.target_types.join(","));
    let _ = writeln!(out, "knowledge_levels: {}", data.knowledge_levels.join(","));
    let budgets_line = data
        .query_budgets
        .iter()
        .map(|b| b.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let _ = writeln!(out, "query_budgets: {budgets_line}");
    let _ = writeln!(out, "repetitions: {}", data.repetitions);
    if let Some(csv) = &data.csv_path {
        let _ = writeln!(out, "csv_path: {csv}");
    }
    if let Some(json) = &data.json_path {
        let _ = writeln!(out, "json_path: {json}");
    }
    out.push_str("---\n");
    out.push_str("cells:\n");
    for cell in &data.cells {
        let _ = writeln!(
            out,
            "  - attack={a} config={c} epsilon={e} target={t} knowledge={k} budget={b} reps={r} successes={s} blocked={blocked} not_observable={not_observable} inconclusive={inconclusive} rate={sr:.4} median_queries={mq} median_final_size={ms} median_posterior={mp}",
            a = cell.attack_kind,
            c = cell.evaluation_config,
            e = cell
                .epsilon
                .map(|v| format!("{v:.3}"))
                .unwrap_or_else(|| "-".to_string()),
            t = cell.target_type,
            k = cell.knowledge_level,
            b = cell.query_budget,
            r = cell.repetitions,
            s = cell.success_count,
            blocked = cell.blocked_count,
            not_observable = cell.not_observable_count,
            inconclusive = cell.inconclusive_count,
            sr = cell.success_rate,
            mq = cell
                .median_queries_to_success
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "-".to_string()),
            ms = cell
                .median_final_candidate_size
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "-".to_string()),
            mp = cell
                .median_final_posterior
                .map(|v| format!("{v:.4}"))
                .unwrap_or_else(|| "-".to_string()),
        );
    }
    out
}

fn render_attack_sweep_pretty(mode: OutputMode, data: &AttackSweepData) -> String {
    let t = title(mode, "check-attack sweep");
    let mut out = format!("{t}\n\n");
    let _ = writeln!(out, "{}", key_value(mode, "started_at", &data.started_at));
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "min_cohort", &data.min_cohort.to_string())
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(
            mode,
            "default_epsilon",
            &format!("{:.4}", data.default_epsilon),
        )
    );
    let _ = writeln!(out, "{}", key_value(mode, "input_dir", &data.input_dir));
    let _ = writeln!(out, "{}", key_value(mode, "as_of_date", &data.as_of_date));
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "attacks", &data.attacks.join(","))
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "configs", &data.configs.join(","))
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(
            mode,
            "epsilons",
            &data
                .epsilons
                .iter()
                .map(|e| format!("{e:.3}"))
                .collect::<Vec<_>>()
                .join(","),
        )
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "target_types", &data.target_types.join(","))
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "knowledge_levels", &data.knowledge_levels.join(","))
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(
            mode,
            "query_budgets",
            &data
                .query_budgets
                .iter()
                .map(|b| b.to_string())
                .collect::<Vec<_>>()
                .join(","),
        )
    );
    let _ = writeln!(
        out,
        "{}",
        key_value(mode, "repetitions", &data.repetitions.to_string())
    );
    if let Some(csv) = &data.csv_path {
        let _ = writeln!(out, "{}", key_value(mode, "csv_path", csv));
    }
    if let Some(json) = &data.json_path {
        let _ = writeln!(out, "{}", key_value(mode, "json_path", json));
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", section_header(mode, "Sweep Cells"));

    for group in group_sweep_cells(&data.cells) {
        render_sweep_group_pretty(&mut out, mode, &group);
    }
    out
}

struct SweepCellGroup<'a> {
    attack: &'a str,
    config: &'a str,
    cells: Vec<&'a AttackSweepCellData>,
}

// Group cells by (attack, config) while preserving the order in which each
// pair first appears. `data.cells` is already sorted by attack -> config ->
// epsilon -> target -> knowledge -> budget (see summarize() in sweep.rs), so
// this just chunks the flat list into contiguous (attack, config) sections.
fn group_sweep_cells(cells: &[AttackSweepCellData]) -> Vec<SweepCellGroup<'_>> {
    let mut groups: Vec<SweepCellGroup<'_>> = Vec::new();
    for cell in cells {
        let attack = cell.attack_kind.as_str();
        let config = cell.evaluation_config.as_str();
        match groups.last_mut() {
            Some(g) if g.attack == attack && g.config == config => g.cells.push(cell),
            _ => groups.push(SweepCellGroup {
                attack,
                config,
                cells: vec![cell],
            }),
        }
    }
    groups
}

fn classify_cell(cell: &AttackSweepCellData) -> &'static str {
    if cell.success_rate >= 0.5 {
        "failed"
    } else if cell.success_rate > 0.0 {
        "borderline"
    } else {
        "passed"
    }
}

fn render_sweep_group_pretty(out: &mut String, mode: OutputMode, group: &SweepCellGroup<'_>) {
    let total = group.cells.len();
    let failed = group
        .cells
        .iter()
        .filter(|c| classify_cell(c) == "failed")
        .count();
    let borderline = group
        .cells
        .iter()
        .filter(|c| classify_cell(c) == "borderline")
        .count();
    let passed = total - failed - borderline;

    let group_status = if failed > 0 {
        "failed"
    } else if borderline > 0 {
        "borderline"
    } else {
        "passed"
    };
    let group_badge = status_badge(mode, group_status);

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  {BOLD}{CYAN}{attack}{RESET} {DARK_GRAY}/{RESET} {BOLD}{MAGENTA}{config}{RESET}  {group_badge}",
        attack = group.attack,
        config = group.config,
    );
    let _ = writeln!(
        out,
        "    {DARK_GRAY}▸{RESET} {DIM}{total} cells:{RESET} {GREEN}{passed} passed{RESET}{DARK_GRAY},{RESET} {YELLOW}{borderline} borderline{RESET}{DARK_GRAY},{RESET} {RED}{failed} failed{RESET}",
    );

    for cell in &group.cells {
        render_sweep_cell_pretty(out, mode, cell);
    }
}

fn render_sweep_cell_pretty(out: &mut String, mode: OutputMode, cell: &AttackSweepCellData) {
    let cell_badge = status_badge(mode, classify_cell(cell));
    let eps = cell
        .epsilon
        .map(|v| format!("{v:.3}"))
        .unwrap_or_else(|| "-".to_string());
    let median_queries = cell
        .median_queries_to_success
        .map(|v| format!("{v:.2}"))
        .unwrap_or_else(|| "-".to_string());
    let median_size = cell
        .median_final_candidate_size
        .map(|v| format!("{v:.2}"))
        .unwrap_or_else(|| "-".to_string());
    let median_post = cell
        .median_final_posterior
        .map(|v| format!("{v:.4}"))
        .unwrap_or_else(|| "-".to_string());

    let eps_pair = axis_pair("eps", &eps, EPSILON_COLOR);
    let target_pair = axis_pair("target", &cell.target_type, TARGET_COLOR);
    let knowledge_pair = axis_pair("knowledge", &cell.knowledge_level, KNOWLEDGE_COLOR);
    let budget_pair = axis_pair("budget", &cell.query_budget.to_string(), BUDGET_COLOR);

    let _ = writeln!(
        out,
        "      {DARK_GRAY}•{RESET} {eps_pair}  {target_pair}  {knowledge_pair}  {budget_pair}  {cell_badge}",
    );
    let _ = writeln!(
        out,
        "          {DARK_GRAY}◦{RESET} {DIM}repetitions={reps} successes={succ} blocked={blocked} not_observable={not_observable} inconclusive={inconclusive} rate={rate:.4}{RESET}",
        reps = cell.repetitions,
        succ = cell.success_count,
        blocked = cell.blocked_count,
        not_observable = cell.not_observable_count,
        inconclusive = cell.inconclusive_count,
        rate = cell.success_rate,
    );
    let _ = writeln!(
        out,
        "          {DARK_GRAY}◦{RESET} {DIM}median_queries_to_success={median_queries}  median_final_size={median_size}  median_posterior={median_post}{RESET}",
    );
}
