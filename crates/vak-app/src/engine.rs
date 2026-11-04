//! The interpretability engine. It changes the prompt in small ways, asks the audited model each time,
//! takes the prompt apart word by word, checks the model's own account of its answer, and rolls
//! everything up into a [`Report`].
//!
//! To save calls it works in two waves. The first wave runs one test of each kind; a kind only gets its
//! remaining tests when the first came back less than clean (or when the effort level asks for everything).
//! A steady model is therefore cleared with a handful of calls, and the budget goes where the trouble is.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;
use thiserror::Error;
use tokio::sync::Semaphore;
use vak_core::audit::importance::{self, UnitSpec};
use vak_core::audit::perturb;
use vak_core::audit::scoring;
use vak_core::audit::{faithfulness, AuditType, CallStats, Preset, Report, Status, TestKind, TestResult, Variant};
use vak_core::lang::Lang;
use vak_core::rng::Rng;
use vak_core::text::{extract_json, lexical_similarity, lexical_verdict, settled_without_judge, Verdict};

use crate::catalog::{chat_cost_inr, TRANSLATE_INR_PER_1K_CHARS};
use crate::judge::Judge;
use crate::plan::AuditRequest;
use crate::ports::{emit, ApiError, ChatReply, ChatRequest, Emit, Llm, Msg, TranslateOpts, Translator};
use crate::target::{Answer, Target};

pub const RESAMPLE_TEMPERATURE: f32 = 0.8;

#[derive(Debug, Error)]
pub enum AuditError {
    /// Something to tell the person in plain words.
    #[error("{0}")]
    Failed(String),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub plan_model: String,
    pub judge_model: String,
    pub explain_model: String,
    pub target_model: String,
    pub parallel: usize,
    pub max_prompt_chars: usize,
    pub call_timeout: Duration,
    pub run_timeout: Duration,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            plan_model: "sarvam-105b".into(),
            judge_model: "sarvam-105b".into(),
            explain_model: "sarvam-105b".into(),
            target_model: "sarvam-105b-conversations".into(),
            parallel: 6,
            max_prompt_chars: 600,
            call_timeout: Duration::from_secs(120),
            run_timeout: Duration::from_secs(480),
        }
    }
}

pub struct Engine {
    pub llm: Arc<dyn Llm>,
    pub translator: Arc<dyn Translator>,
    pub settings: Settings,
}

struct Run {
    target: Arc<dyn Target>,
    tx: Emit,
    sem: Semaphore,
    fatal: Mutex<Option<ApiError>>,
    notes: Mutex<Vec<String>>,
    usage: Mutex<HashMap<String, (u64, u64)>>,
    chars_translated: AtomicUsize,
    target_calls: AtomicUsize,
    helper_calls: AtomicUsize,
    cached: AtomicUsize,
    done: AtomicUsize,
    total: AtomicUsize,
    call_timeout: Duration,
}

impl Run {
    fn note(&self, s: impl Into<String>) {
        self.notes.lock().expect("notes").push(s.into());
    }

    fn record(&self, model: &str, r: &ChatReply) {
        let mut u = self.usage.lock().expect("usage");
        let e = u.entry(model.to_string()).or_default();
        e.0 += r.tokens_in;
        e.1 += r.tokens_out;
        if r.cached {
            self.cached.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn check_fatal(&self) -> Result<(), ApiError> {
        match self.fatal.lock().expect("fatal").clone() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Ask the audited model. Auth and credit failures abort the run; everything else becomes an error card.
    async fn ask(&self, text: &str, temperature: f32, n: u32) -> Result<Vec<Answer>, ApiError> {
        self.check_fatal()?;
        let _permit = self.sem.acquire().await.map_err(|_| ApiError::Other("closed".into()))?;
        self.check_fatal()?;
        self.target_calls.fetch_add(1, Ordering::Relaxed);
        let fail = |m: &str| (0..n).map(|_| Answer::failed(m)).collect::<Vec<_>>();
        match tokio::time::timeout(self.call_timeout, self.target.ask(text, temperature, n, 420)).await {
            Err(_) => Ok(fail("The model took too long to answer.")),
            Ok(Err(e)) if e.is_fatal() => {
                *self.fatal.lock().expect("fatal") = Some(e.clone());
                Err(e)
            }
            Ok(Err(e)) => Ok(fail(&e.to_string())),
            Ok(Ok(answers)) => {
                if let Some(m) = self.target.billed_model() {
                    let mut u = self.usage.lock().expect("usage");
                    let e = u.entry(m).or_default();
                    for a in &answers {
                        e.0 += a.tokens_in;
                        e.1 += a.tokens_out;
                    }
                }
                self.cached.fetch_add(answers.iter().filter(|a| a.cached).count(), Ordering::Relaxed);
                Ok(answers)
            }
        }
    }

    fn tick(&self) {
        let done = self.done.fetch_add(1, Ordering::Relaxed) + 1;
        emit(&self.tx, "progress", json!({"done": done, "total": self.total.load(Ordering::Relaxed)}));
    }
}

type Verdicts = HashMap<String, (Verdict, String, bool)>;

impl Engine {
    pub async fn run(&self, req: &AuditRequest, target: Arc<dyn Target>, tx: Emit) -> Result<Report, AuditError> {
        let run = Run {
            target,
            tx,
            sem: Semaphore::new(self.settings.parallel.max(1)),
            fatal: Mutex::new(None),
            notes: Mutex::new(req.plan.notes.clone()),
            usage: Mutex::new(HashMap::new()),
            chars_translated: AtomicUsize::new(0),
            target_calls: AtomicUsize::new(0),
            helper_calls: AtomicUsize::new(0),
            cached: AtomicUsize::new(0),
            done: AtomicUsize::new(0),
            total: AtomicUsize::new(0),
            call_timeout: self.settings.call_timeout,
        };
        let started = Instant::now();
        let mut report = match tokio::time::timeout(self.settings.run_timeout, self.run_inner(&run, req)).await {
            Ok(r) => r?,
            Err(_) => return Err(AuditError::Failed("This audit was taking too long, so I stopped it. Try a lighter effort or a shorter question.".into())),
        };
        report.seconds = (started.elapsed().as_secs_f64() * 10.0).round() / 10.0;
        let usage = run.usage.lock().expect("usage").clone();
        report.cost_inr = (usage.iter().map(|(m, (i, o))| chat_cost_inr(m, *i, *o)).sum::<f64>()
            + run.chars_translated.load(Ordering::Relaxed) as f64 / 1000.0 * TRANSLATE_INR_PER_1K_CHARS)
            * 100.0;
        report.cost_inr = report.cost_inr.round() / 100.0;
        report.calls = CallStats {
            target: run.target_calls.load(Ordering::Relaxed),
            helpers: run.helper_calls.load(Ordering::Relaxed),
            cached: run.cached.load(Ordering::Relaxed),
            planned: report.calls.planned,
            tokens_in: usage.values().map(|u| u.0).sum(),
            tokens_out: usage.values().map(|u| u.1).sum(),
        };
        Ok(report)
    }

    async fn helper(&self, run: &Run, model: &str, req: ChatRequest) -> Result<ChatReply, ApiError> {
        run.check_fatal()?;
        run.helper_calls.fetch_add(1, Ordering::Relaxed);
        let r = match tokio::time::timeout(self.settings.call_timeout, self.llm.chat(req)).await {
            Err(_) => return Err(ApiError::Timeout),
            Ok(r) => r?,
        };
        run.record(model, &r);
        Ok(r)
    }

    async fn run_inner(&self, run: &Run, req: &AuditRequest) -> Result<Report, AuditError> {
        let plan = &req.plan;
        let prompt = plan.prompt.as_str();
        let lang = Lang::get(&plan.prompt_language).unwrap_or_else(Lang::english);
        let preset = plan.effort.preset();
        let types: HashSet<AuditType> = plan.audit_types.iter().copied().collect();

        emit(&run.tx, "phase", json!({"name": "variants"}));
        let variants = self.variants(run, prompt, lang, &preset, &types).await?;
        let want_units = types.contains(&AuditType::Perturbation) || types.contains(&AuditType::Behavioral);
        let units = if want_units { importance::make_units(prompt, preset.units) } else { vec![] };
        let resample_cap = if types.contains(&AuditType::Consistency) { preset.resamples } else { 0 };
        let first_resamples = if preset.adaptive { resample_cap.min(2) } else { resample_cap };

        // wave 1 holds `first_wave` of every kind; the rest wait on standby
        let mut taken: HashMap<&str, usize> = HashMap::new();
        let (mut wave1, mut standby) = (vec![], vec![]);
        for v in &variants {
            let n = taken.entry(v.category.as_str()).or_insert(0);
            if *n < preset.first_wave { wave1.push(v.clone()) } else { standby.push(v.clone()) }
            *n += 1;
        }
        emit(
            &run.tx,
            "plan",
            json!({
                "variants": variants.iter().map(|v| json!({"id": v.id, "category": v.category, "kind": v.kind, "label": v.label, "input": v.text,
                    "standby": standby.iter().any(|s| s.id == v.id)})).collect::<Vec<_>>(),
                "units": units.iter().map(|u| json!({"index": u.index, "text": u.text, "start": u.start, "end": u.end})).collect::<Vec<_>>(),
                "resamples": resample_cap,
                "effort": plan.effort.as_str(),
            }),
        );
        if variants.is_empty() && resample_cap == 0 {
            return Err(AuditError::Failed("There was nothing to test for the checks you picked. Try a longer question or another check.".into()));
        }

        emit(&run.tx, "phase", json!({"name": "baseline"}));
        let first = run.ask(prompt, 0.0, 1).await?.remove(0);
        if !first.usable() {
            let why = first.error.clone().unwrap_or_else(|| "it sent back nothing".into());
            return Err(AuditError::Failed(format!("The model didn't answer your question ({why}), so there's nothing to compare against.")));
        }
        let baseline = first.text.clone();
        emit(&run.tx, "baseline", json!({"output": baseline}));

        let probing = !units.is_empty() && run.target.can_explain();
        let planned = variants.len() + units.len() + (resample_cap > 0) as usize + probing as usize;
        run.total.store(wave1.len() + units.len() + (first_resamples > 0) as usize + probing as usize, Ordering::Relaxed);
        emit(&run.tx, "progress", json!({"done": 0, "total": run.total.load(Ordering::Relaxed)}));

        emit(&run.tx, "phase", json!({"name": "probe"}));
        let mut answers = Answers::default();
        self.probe_wave(run, prompt, &wave1, &units, first_resamples, probing, &mut answers).await?;

        emit(&run.tx, "phase", json!({"name": "compare"}));
        let mut verdicts: Verdicts = HashMap::new();
        let mut judge_used = true;
        let mut ran: Vec<Variant> = wave1.clone();
        let mut outputs = answers.outputs(&ran, &units, plan.reference_output.as_deref());
        judge_used &= self.compare(run, prompt, &baseline, &outputs, &mut verdicts, plan.effort.preset().judge_reasoning).await?;

        // wave 2: more of whatever kind showed trouble
        if preset.adaptive && (!standby.is_empty() || resample_cap > first_resamples) {
            let troubled: HashSet<String> = ran
                .iter()
                .filter(|v| verdicts.get(&v.id).is_some_and(|(vd, _, _)| status_of(v.kind, *vd) != Status::Ok))
                .map(|v| v.category.clone())
                .collect();
            let extra: Vec<Variant> = standby.iter().filter(|v| troubled.contains(&v.category)).cloned().collect();
            let resample_trouble = (1..=answers.resamples.len())
                .any(|i| verdicts.get(&format!("resample-{i}")).is_some_and(|(vd, _, _)| status_of(TestKind::Resample, *vd) != Status::Ok));
            let more_resamples = if resample_trouble { resample_cap - first_resamples } else { 0 };
            if !extra.is_empty() || more_resamples > 0 {
                run.total.fetch_add(extra.len() + (more_resamples > 0) as usize, Ordering::Relaxed);
                emit(&run.tx, "phase", json!({"name": "probe"}));
                let mut more = Answers::default();
                self.probe_wave(run, prompt, &extra, &[], more_resamples, false, &mut more).await?;
                answers.merge(more);
                ran.extend(extra);
                outputs = answers.outputs(&ran, &units, plan.reference_output.as_deref());
                let rest: Vec<(String, String)> = outputs.iter().filter(|(k, _)| !verdicts.contains_key(k)).cloned().collect();
                judge_used &= self.compare(run, prompt, &baseline, &rest, &mut verdicts, preset.judge_reasoning).await?;
            }
        }

        emit(&run.tx, "phase", json!({"name": "score"}));
        let mut tests = vec![];
        for v in &ran {
            tests.push(build_test(&v.id, &v.category, v.kind, &v.label, &v.text, answers.variants.get(&v.id), &v.note, &verdicts));
        }
        for (i, a) in answers.resamples.iter().enumerate() {
            let id = format!("resample-{}", i + 1);
            tests.push(build_test(&id, "resample", TestKind::Resample, &format!("Asked again (try {})", i + 1), prompt, Some(a), "", &verdicts));
        }
        for t in &tests {
            emit(&run.tx, "test", serde_json::to_value(t).unwrap_or_default());
        }
        let raws = unit_raws(&units, &baseline, &outputs, &verdicts);
        let ranked = importance::rank_units(&units, &raws);
        emit(&run.tx, "units", json!({"units": ranked}));

        let faith = match (&answers.probe, ranked.is_empty()) {
            (Some(p), false) if p.error.is_none() => faithfulness::evaluate(&faithfulness::parse_key_line(&p.text).1, &ranked),
            _ => vo_skipped(),
        };
        let (score, total, unexpected, skipped) = scoring::consistency(&tests);
        if total == 0 {
            return Err(AuditError::Failed("None of the tests came back with an answer I could compare. Please try again in a moment.".into()));
        }
        let reference_agrees = plan.reference_output.as_ref().and(verdicts.get("reference")).map(|(v, _, _)| *v != Verdict::Different);
        let groups = scoring::group_scores(&tests);
        let flags = scoring::build_flags(&tests, &ranked, &faith, &baseline, lang, reference_agrees);
        if skipped > 0 {
            run.note(format!("{skipped} test{} got no answer and {} left out of the score.", if skipped == 1 { "" } else { "s" }, if skipped == 1 { "was" } else { "were" }));
        }
        if !judge_used && tests.iter().any(|t| t.status != Status::Error && !t.judged) {
            run.note("The comparison helper wasn't available, so answers were compared by wording alone. Treat the score as rougher than usual.");
        }
        if preset.adaptive && ran.len() + answers.resamples.len() < planned.saturating_sub(units.len() + probing as usize) {
            run.note("Kinds of tests that came back clean were not repeated, to save calls. Choose Thorough to run everything.");
        }
        let nt_codes = scoring::not_tested_codes(&plan.audit_types, plan.effort, &groups);
        let next_codes = scoring::next_step_codes(&flags);
        Ok(Report {
            prompt: prompt.to_string(),
            prompt_language: lang.code.clone(),
            baseline_output: baseline,
            reference_output: plan.reference_output.clone(),
            target_label: run.target.label(),
            depth: plan.effort.depth_name().into(),
            effort: plan.effort.as_str().into(),
            audit_types: plan.audit_types.clone(),
            consistency_score: score,
            grade: scoring::grade_for(score),
            total_perturbations: total,
            unexpected_changes: unexpected,
            skipped,
            tests,
            groups: groups.clone(),
            units: ranked,
            faithfulness: faith,
            flags: flags.clone(),
            strengths: scoring::strengths(&groups),
            next_steps: scoring::next_steps(&flags),
            next_step_codes: next_codes,
            not_tested: scoring::not_tested(&nt_codes),
            not_tested_codes: nt_codes,
            notes: run.notes.lock().expect("notes").clone(),
            judge_used,
            cost_inr: 0.0,
            seconds: 0.0,
            calls: CallStats { planned, ..Default::default() },
        })
    }

    async fn variants(&self, run: &Run, prompt: &str, lang: Lang, p: &Preset, types: &HashSet<AuditType>) -> Result<Vec<Variant>, AuditError> {
        let mut rng = Rng::from_text("perturb", prompt);
        let mut found: Vec<Variant> = vec![];
        let perturb_on = types.contains(&AuditType::Perturbation);
        let behave_on = types.contains(&AuditType::Behavioral);
        if perturb_on {
            found.extend(perturb::typo_variants(prompt, p.typo, &mut rng, lang));
            found.extend(perturb::format_variants(prompt, lang, p.format));
            found.extend(perturb::numeral_variant(prompt, lang));
            found.extend(perturb::distractor_variants(prompt, lang, p.distractor));
            found.extend(perturb::polite_variants(prompt, lang, p.polite));
        }
        if behave_on {
            found.extend(perturb::control_variants(lang, p.control));
        }
        let (n_para, n_opp) = (if perturb_on { p.paraphrase } else { 0 }, if behave_on { p.negation } else { 0 });
        let rewrite = async {
            if n_para + n_opp == 0 {
                return Ok((vec![], vec![]));
            }
            let req = ChatRequest::new(&self.settings.plan_model, vec![Msg::system(perturb::rewrite_system(n_para, n_opp)), Msg::user(prompt)])
                .json()
                .max_tokens(200 + 160 * (n_para + n_opp) as u32);
            let reply = self.helper(run, &self.settings.plan_model, req).await?;
            Ok::<_, ApiError>(extract_json(reply.text()).map(|j| perturb::rewrite_variants(prompt, lang, &j, n_para, n_opp)).unwrap_or_default())
        };
        let script = async {
            if !perturb_on || lang.script == vak_core::text::Script::Latin || lang.tier != vak_core::lang::Tier::Full {
                return Ok(None);
            }
            run.chars_translated.fetch_add(prompt.chars().count(), Ordering::Relaxed);
            Ok::<_, ApiError>(perturb::roman_variant(prompt, &self.translator.romanize(prompt, &lang.code).await?))
        };
        let language = async {
            let mut out = vec![];
            if !perturb_on {
                return Ok::<_, ApiError>(out);
            }
            let targets = perturb::language_targets(lang, p.language);
            run.chars_translated.fetch_add(prompt.chars().count() * targets.len(), Ordering::Relaxed);
            let opts = TranslateOpts::default();
            let results = futures::future::join_all(targets.iter().map(|dst| self.translator.translate(prompt, &lang.code, &dst.code, &opts))).await;
            for (dst, r) in targets.iter().zip(results) {
                match r {
                    Ok(t) => out.extend(perturb::language_variant(*dst, prompt, &t)),
                    Err(e) if e.is_fatal() => return Err(e),
                    Err(_) => run.note(format!("I couldn't translate into {}.", dst.english)),
                }
            }
            Ok(out)
        };
        let (rw, sc, lg) = tokio::join!(rewrite, script, language);
        for (what, fatal) in [("the rewordings", rw.as_ref().err()), ("the English-letters version", sc.as_ref().err()), ("the other-language versions", lg.as_ref().err())] {
            if let Some(e) = fatal {
                if e.is_fatal() {
                    return Err(e.clone().into());
                }
                run.note(format!("I couldn't prepare {what} this time, so that part was skipped."));
            }
        }
        if let Ok((paras, opps)) = rw {
            if paras.is_empty() && n_para > 0 {
                run.note("No usable rewordings came back, so that group is missing.");
            }
            found.extend(paras);
            found.extend(sc.ok().flatten());
            found.extend(lg.unwrap_or_default());
            found.extend(opps);
        } else {
            found.extend(sc.ok().flatten());
            found.extend(lg.unwrap_or_default());
        }
        let mut seen = vec![prompt.trim().to_string()];
        found.retain(|v| {
            let t = v.text.trim().to_string();
            !t.is_empty() && !seen.contains(&t) && {
                seen.push(t);
                true
            }
        });
        Ok(found)
    }

    #[allow(clippy::too_many_arguments)]
    async fn probe_wave(&self, run: &Run, prompt: &str, variants: &[Variant], units: &[UnitSpec], resamples: usize, probing: bool, out: &mut Answers) -> Result<(), AuditError> {
        let v_jobs = variants.iter().map(|v| async move {
            let a = run.ask(&v.text, 0.0, 1).await?.remove(0);
            emit(&run.tx, "answer", json!({"id": v.id, "output": if a.error.is_some() { None } else { Some(a.text.clone()) }, "error": a.error}));
            run.tick();
            Ok::<_, ApiError>((v.id.clone(), a))
        });
        let u_jobs = units.iter().map(|u| async move {
            let a = run.ask(&importance::without_unit(prompt, u), 0.0, 1).await?.remove(0);
            run.tick();
            Ok::<_, ApiError>((u.index, a))
        });
        let r_job = async {
            if resamples == 0 {
                return Ok(vec![]);
            }
            let a = run.ask(prompt, RESAMPLE_TEMPERATURE, resamples as u32).await?;
            for (i, x) in a.iter().enumerate() {
                emit(&run.tx, "answer", json!({"id": format!("resample-{}", i + 1), "output": if x.error.is_some() { None } else { Some(x.text.clone()) }, "error": x.error}));
            }
            run.tick();
            Ok::<_, ApiError>(a)
        };
        let p_job = async {
            if !probing {
                return Ok(None);
            }
            let a = run.ask(&format!("{prompt}{}", faithfulness::PROBE_SUFFIX), 0.0, 1).await?.remove(0);
            run.tick();
            Ok::<_, ApiError>(Some(a))
        };
        let (vs, us, rs, ps) = tokio::join!(futures::future::join_all(v_jobs), futures::future::join_all(u_jobs), r_job, p_job);
        for r in vs {
            let (id, a) = r?;
            out.variants.insert(id, a);
        }
        for r in us {
            let (i, a) = r?;
            out.units.insert(i, a);
        }
        out.resamples = rs?;
        out.probe = ps?;
        Ok(())
    }

    /// Cheap checks first; the judge for what is left. Returns whether the judge answered when it was needed.
    async fn compare(&self, run: &Run, task: &str, baseline: &str, outputs: &[(String, String)], verdicts: &mut Verdicts, reasoning: Option<&'static str>) -> Result<bool, AuditError> {
        let mut undecided: Vec<(String, String)> = vec![];
        for (k, t) in outputs {
            if verdicts.contains_key(k) {
                continue;
            }
            if settled_without_judge(baseline, t) {
                verdicts.insert(k.clone(), (Verdict::Same, String::new(), false));
            } else {
                undecided.push((k.clone(), t.clone()));
            }
        }
        let mut judged = HashMap::new();
        if !undecided.is_empty() {
            let judge = Judge { llm: self.llm.clone(), model: self.settings.judge_model.clone(), reasoning: reasoning.map(String::from), batch_size: 10 };
            run.helper_calls.fetch_add(undecided.len().div_ceil(10), Ordering::Relaxed);
            match tokio::time::timeout(self.settings.call_timeout * 2, judge.compare(task, baseline, &undecided)).await {
                Ok(Ok(j)) => {
                    for r in &j.usage {
                        run.record(&self.settings.judge_model, r);
                    }
                    judged = j.verdicts;
                }
                Ok(Err(e)) if e.is_fatal() => return Err(e.into()),
                _ => {}
            }
        }
        let had_undecided = !undecided.is_empty();
        for (k, t) in undecided {
            match judged.remove(&k) {
                Some((v, why)) => verdicts.insert(k, (v, why, true)),
                None => verdicts.insert(k, (lexical_verdict(baseline, &t).0, String::new(), false)),
            };
        }
        Ok(!had_undecided || !verdicts.values().any(|(_, _, j)| !j) || verdicts.values().any(|(_, _, j)| *j))
    }
}

fn vo_skipped() -> vak_core::audit::Faithfulness {
    vak_core::audit::Faithfulness::skipped("Not run for this audit.")
}

#[derive(Default)]
struct Answers {
    variants: HashMap<String, Answer>,
    units: HashMap<usize, Answer>,
    resamples: Vec<Answer>,
    probe: Option<Answer>,
}

impl Answers {
    fn merge(&mut self, more: Answers) {
        self.variants.extend(more.variants);
        self.resamples.extend(more.resamples);
    }

    /// (id, text) of everything that came back usable, in a stable order.
    fn outputs(&self, variants: &[Variant], units: &[UnitSpec], reference: Option<&str>) -> Vec<(String, String)> {
        let mut out = vec![];
        for v in variants {
            if let Some(a) = self.variants.get(&v.id).filter(|a| a.usable()) {
                out.push((v.id.clone(), a.text.clone()));
            }
        }
        for (i, a) in self.resamples.iter().enumerate().filter(|(_, a)| a.usable()) {
            out.push((format!("resample-{}", i + 1), a.text.clone()));
        }
        for u in units {
            if let Some(a) = self.units.get(&u.index).filter(|a| a.usable()) {
                out.push((format!("unit-{}", u.index), a.text.clone()));
            }
        }
        if let Some(r) = reference {
            out.push(("reference".into(), r.to_string()));
        }
        out
    }
}

fn status_of(kind: TestKind, v: Verdict) -> Status {
    let mut t = TestResult::new("x", "x", kind, "", "", "");
    t.output = Some(String::new());
    t.sim = Some(v.score());
    scoring::finish_test(&mut t);
    t.status
}

#[allow(clippy::too_many_arguments)]
fn build_test(id: &str, category: &str, kind: TestKind, label: &str, text: &str, ans: Option<&Answer>, note: &str, verdicts: &Verdicts) -> TestResult {
    let mut t = TestResult::new(id, category, kind, label, text, note);
    if let Some(a) = ans {
        if a.usable() {
            if let Some((v, why, judged)) = verdicts.get(id) {
                t.output = Some(a.text.clone());
                t.verdict = Some(*v);
                t.sim = Some(v.score());
                t.reason = why.clone();
                t.judged = *judged;
            }
        } else if let Some(e) = &a.error {
            t.reason = e.clone();
        }
    }
    scoring::finish_test(&mut t);
    t
}

fn unit_raws(units: &[UnitSpec], baseline: &str, outputs: &[(String, String)], verdicts: &Verdicts) -> HashMap<usize, f64> {
    let mut raws = HashMap::new();
    for u in units {
        let key = format!("unit-{}", u.index);
        if let (Some((_, text)), Some((v, _, _))) = (outputs.iter().find(|(k, _)| *k == key), verdicts.get(&key)) {
            raws.insert(u.index, 0.8 * (1.0 - v.score()) + 0.2 * (1.0 - lexical_similarity(baseline, text)));
        }
    }
    raws
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakes::*;
    use crate::plan::{AuditRequest, Plan};
    use crate::target::PracticeTarget;
    use vak_core::audit::Effort;

    const Q: &str = "ఈ సినిమా చాలా బాగుంది, కథ అద్భుతం.";

    async fn audit(steady: bool, effort: Effort) -> (Report, Vec<(String, serde_json::Value)>) {
        let engine = Engine { llm: Arc::new(FakeLlm::default()), translator: Arc::new(FakeTranslator), settings: Settings::default() };
        let plan = Plan { prompt: Q.into(), reference_output: None, target: Default::default(), audit_types: vec![], effort, prompt_language: "te".into(), notes: vec![], source_text: String::new() }
            .normalised()
            .unwrap();
        let req = AuditRequest { plan, language: "te".into(), voice: false, voice_id: None, fresh: false };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let report = engine.run(&req, Arc::new(PracticeTarget { steady }), tx).await.expect("audit runs");
        let mut events = vec![];
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        (report, events)
    }

    #[tokio::test]
    async fn steady_model_scores_high_and_shaky_scores_lower() {
        let (steady, _) = audit(true, Effort::Balanced).await;
        let (shaky, _) = audit(false, Effort::Balanced).await;
        assert!(steady.consistency_score > 0.8, "steady {} {:?}", steady.consistency_score, steady.tests.iter().filter(|t| t.status != Status::Ok).map(|t| (t.id.clone(), t.input.clone(), t.output.clone())).collect::<Vec<_>>());
        assert!(shaky.consistency_score < steady.consistency_score, "{} vs {}", shaky.consistency_score, steady.consistency_score);
        assert!(shaky.flags.iter().any(|f| f.code == "distraction_sensitive" || f.code == "paraphrase_flip"), "{:?}", shaky.flags.iter().map(|f| &f.code).collect::<Vec<_>>());
        assert!(shaky.unexpected_changes > steady.unexpected_changes);
    }

    #[tokio::test]
    async fn clean_kinds_are_not_repeated_but_thorough_runs_everything() {
        let (balanced, _) = audit(true, Effort::Balanced).await;
        let (thorough, _) = audit(true, Effort::Thorough).await;
        assert!(balanced.calls.target < thorough.calls.target, "{} vs {}", balanced.calls.target, thorough.calls.target);
        assert!(balanced.tests.len() < thorough.tests.len());
        assert!(balanced.notes.iter().any(|n| n.contains("not repeated")));
        assert!(!thorough.notes.iter().any(|n| n.contains("not repeated")));
    }

    #[tokio::test]
    async fn trouble_pulls_in_the_held_back_tests() {
        let (shaky, events) = audit(false, Effort::Balanced).await;
        let distract = shaky.tests.iter().filter(|t| t.category == "distractor").count();
        assert_eq!(distract, 2, "the second distractor runs once the first misbehaves");
        let names: Vec<&str> = events.iter().map(|(n, _)| n.as_str()).collect();
        for e in ["phase", "plan", "baseline", "progress", "answer", "test", "units"] {
            assert!(names.contains(&e), "missing event {e}");
        }
    }

    #[tokio::test]
    async fn bad_key_aborts_with_the_api_error() {
        let engine = Engine { llm: Arc::new(FakeLlm { fail: Some(ApiError::Auth), ..Default::default() }), translator: Arc::new(FakeTranslator), settings: Settings::default() };
        let plan = Plan { prompt: Q.into(), reference_output: None, target: Default::default(), audit_types: vec![], effort: Effort::Light, prompt_language: "te".into(), notes: vec![], source_text: String::new() }.normalised().unwrap();
        let req = AuditRequest { plan, language: "te".into(), voice: false, voice_id: None, fresh: false };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let err = engine.run(&req, Arc::new(PracticeTarget { steady: true }), tx).await.unwrap_err();
        assert!(matches!(err, AuditError::Api(ApiError::Auth)), "{err:?}");
    }
}
