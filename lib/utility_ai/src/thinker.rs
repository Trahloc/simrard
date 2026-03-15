//! Thinkers are the "brain" of an entity. You attach Scorers to it, and the
//! Thinker picks the right Action to run based on the resulting Scores.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use bevy::{
    log::{
        tracing::{field, span, Span},
        Level,
    },
    prelude::*,
};

use crate::{
    actions::{self, ActionBuilder, ActionBuilderWrapper, ActionState},
    choices::{Choice, ChoiceBuilder},
    pickers::Picker,
    scorers::{Score, ScorerBuilder},
};

/// Hysteresis: prefer the currently running action when a newly picked action
/// only beats it by a tiny margin. This prevents action flapping.
const HYSTERESIS_EPSILON: f32 = 0.05;

/// Wrapper for Actor entities. In terms of Scorers, Thinkers, and Actions,
/// this is the [`Entity`] actually _performing_ the action, rather than the
/// entity a Scorer/Thinker/Action is attached to. Generally, you will use
/// this entity when writing Queries for Action and Scorer systems.
#[derive(Debug, Clone, Component, Copy, Reflect)]
pub struct Actor(pub Entity);

#[derive(Debug, Clone, Copy, Reflect)]
pub struct Action(pub Entity);

impl Action {
    pub fn entity(&self) -> Entity {
        self.0
    }
}

#[derive(Debug, Clone, Component)]
pub struct ActionSpan {
    pub(crate) span: Span,
}

impl ActionSpan {
    pub(crate) fn new(action: Entity, label: Option<&str>) -> Self {
        let span = span!(
            Level::DEBUG,
            "action",
            ent = ?action,
            label = field::Empty,
        );
        if let Some(label) = label {
            span.record("label", label);
        }
        Self { span }
    }

    pub fn span(&self) -> &Span {
        &self.span
    }
}

#[derive(Debug, Clone, Copy, Reflect)]
pub struct Scorer(pub Entity);

#[derive(Debug, Clone, Component)]
pub struct ScorerSpan {
    pub(crate) span: Span,
}

impl ScorerSpan {
    pub(crate) fn new(scorer: Entity, label: Option<&str>) -> Self {
        let span = span!(
            Level::DEBUG,
            "scorer",
            ent = ?scorer,
            label = field::Empty,
        );

        if let Some(label) = label {
            span.record("label", label);
        }
        Self { span }
    }

    pub fn span(&self) -> &Span {
        &self.span
    }
}

/// The "brains" behind this whole operation. A `Thinker` is what glues
/// together `Actions` and `Scorers` and shapes larger, intelligent-seeming
/// systems.
///
/// Note: Thinkers are also Actions, so anywhere you can pass in an Action (or
/// [`ActionBuilder`]), you can pass in a Thinker (or [`ThinkerBuilder`]).
///
/// ### Example
///
/// ```
/// # use bevy::prelude::*;
/// # use simrard_lib_utility_ai::prelude::*;
/// # #[derive(Component, Debug)]
/// # struct Thirst(f32, f32);
/// # #[derive(Component, Debug)]
/// # struct Hunger(f32, f32);
/// # #[derive(Clone, Component, Debug, ScorerBuilder)]
/// # struct Thirsty;
/// # #[derive(Clone, Component, Debug, ScorerBuilder)]
/// # struct Hungry;
/// # #[derive(Clone, Component, Debug, ActionBuilder)]
/// # struct Drink;
/// # #[derive(Clone, Component, Debug, ActionBuilder)]
/// # struct Eat;
/// # #[derive(Clone, Component, Debug, ActionBuilder)]
/// # struct Meander;
/// pub fn init_entities(mut cmd: Commands) {
///     cmd.spawn((
///         Thirst(70.0, 2.0),
///         Hunger(50.0, 3.0),
///         Thinker::build()
///             .picker(FirstToScore::new(80.0))
///             .when(Thirsty, Drink)
///             .when(Hungry, Eat)
///             .otherwise(Meander),
///     ));
/// }
/// ```
#[derive(Component, Debug, Reflect)]
#[reflect(from_reflect = false)]
pub struct Thinker {
    #[reflect(ignore)]
    picker: Arc<dyn Picker>,
    #[reflect(ignore)]
    otherwise: Option<ActionBuilderWrapper>,
    #[reflect(ignore)]
    choices: Vec<Choice>,
    #[reflect(ignore)]
    current_action: Option<(Action, ActionBuilderWrapper)>,
    current_action_label: Option<Option<String>>,
    #[reflect(ignore)]
    span: Span,
    #[reflect(ignore)]
    scheduled_actions: VecDeque<ActionBuilderWrapper>,
    decision_tick: u64,
    repeat_action_cooldown_ticks: u64,
    #[reflect(ignore)]
    completed_action_tick: HashMap<usize, u64>,
}

impl Thinker {
    /// Make a new [`ThinkerBuilder`]. This is what you'll actually use to
    /// configure Thinker behavior.
    pub fn build() -> ThinkerBuilder {
        ThinkerBuilder::new()
    }

    pub fn schedule_action(&mut self, action: impl ActionBuilder + 'static) {
        self.scheduled_actions
            .push_back(ActionBuilderWrapper::new(Arc::new(action)));
    }
}

/// This is what you actually use to configure Thinker behavior. It's a plain
/// old [`ActionBuilder`], as well.
#[derive(Component, Clone, Debug, Default)]
pub struct ThinkerBuilder {
    picker: Option<Arc<dyn Picker>>,
    otherwise: Option<ActionBuilderWrapper>,
    choices: Vec<ChoiceBuilder>,
    label: Option<String>,
    repeat_action_cooldown_ticks: u64,
}

impl ThinkerBuilder {
    pub(crate) fn new() -> Self {
        Self {
            picker: None,
            otherwise: None,
            choices: Vec::new(),
            label: None,
            repeat_action_cooldown_ticks: 0,
        }
    }

    /// Define a [`Picker`](crate::pickers::Picker) for this Thinker.
    pub fn picker(mut self, picker: impl Picker + 'static) -> Self {
        self.picker = Some(Arc::new(picker));
        self
    }

    /// Define an [`ActionBuilder`](crate::actions::ActionBuilder) and
    /// [`ScorerBuilder`](crate::scorers::ScorerBuilder) pair.
    pub fn when(
        mut self,
        scorer: impl ScorerBuilder + 'static,
        action: impl ActionBuilder + 'static,
    ) -> Self {
        self.choices
            .push(ChoiceBuilder::new(Arc::new(scorer), Arc::new(action)));
        self
    }

    /// Default `Action` to execute if the `Picker` did not pick any of the
    /// given choices.
    pub fn otherwise(mut self, otherwise: impl ActionBuilder + 'static) -> Self {
        self.otherwise = Some(ActionBuilderWrapper::new(Arc::new(otherwise)));
        self
    }

    /// * Configures a label to use for the thinker when logging.
    pub fn label(mut self, label: impl AsRef<str>) -> Self {
        self.label = Some(label.as_ref().to_string());
        self
    }

    /// Prevent immediate reselection of the same action after completion.
    /// Measured in thinker decision ticks; `0` disables cooldown.
    pub fn repeat_action_cooldown_ticks(mut self, ticks: u64) -> Self {
        self.repeat_action_cooldown_ticks = ticks;
        self
    }
}

impl ActionBuilder for ThinkerBuilder {
    fn build(&self, cmd: &mut Commands, action_ent: Entity, actor: Entity) {
        let span = span!(
            Level::DEBUG,
            "thinker",
            actor = ?actor,
        );
        let _guard = span.enter();
        debug!("Spawning Thinker.");
        let choices = self
            .choices
            .iter()
            .map(|choice| choice.build(cmd, actor, action_ent))
            .collect();
        std::mem::drop(_guard);
        cmd.entity(action_ent)
            .insert(Thinker {
                picker: self
                    .picker
                    .clone()
                    .expect("ThinkerBuilder requires an explicit Picker via .picker(...)."),
                otherwise: self.otherwise.clone(),
                choices,
                current_action: None,
                current_action_label: None,
                span,
                scheduled_actions: VecDeque::new(),
                decision_tick: 0,
                repeat_action_cooldown_ticks: self.repeat_action_cooldown_ticks,
                completed_action_tick: HashMap::new(),
            })
            .insert(Name::new("Thinker"))
            .insert(ActionState::Requested);
    }

    fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
}

pub fn thinker_component_attach_system(
    mut cmd: Commands,
    q: Query<(Entity, &ThinkerBuilder), Without<HasThinker>>,
) {
    for (entity, thinker_builder) in q.iter() {
        let thinker = actions::spawn_action(thinker_builder, &mut cmd, entity);
        cmd.entity(entity).insert(HasThinker(thinker));
    }
}

pub fn thinker_component_detach_system(
    mut cmd: Commands,
    q: Query<(Entity, &HasThinker), Without<ThinkerBuilder>>,
) {
    for (actor, HasThinker(thinker)) in q.iter() {
        if let Ok(mut ent) = cmd.get_entity(*thinker) {
            ent.despawn();
        }
        cmd.entity(actor).remove::<HasThinker>();
    }
}

pub fn actor_gone_cleanup(
    mut cmd: Commands,
    actors: Query<&ThinkerBuilder>,
    q: Query<(Entity, &Actor)>,
) {
    for (child, Actor(actor)) in q.iter() {
        if actors.get(*actor).is_err() {
            // Actor is gone. Let's clean up.
            if let Ok(mut ent) = cmd.get_entity(child) {
                ent.despawn();
            }
        }
    }
}

#[derive(Component, Debug, Reflect)]
pub struct HasThinker(Entity);

impl HasThinker {
    pub fn entity(&self) -> Entity {
        self.0
    }
}

pub struct ThinkerIterations {
    index: usize,
    max_duration: Duration,
}
impl ThinkerIterations {
    pub fn new(max_duration: Duration) -> Self {
        Self {
            index: 0,
            max_duration,
        }
    }
}
impl Default for ThinkerIterations {
    fn default() -> Self {
        Self::new(Duration::from_millis(10))
    }
}

pub fn thinker_system(
    mut cmd: Commands,
    mut iterations: Local<ThinkerIterations>,
    mut thinker_q: Query<(Entity, &Actor, &mut Thinker)>,
    scores: Query<&Score>,
    mut action_states: Query<&mut actions::ActionState>,
    action_spans: Query<&ActionSpan>,
    scorer_spans: Query<&ScorerSpan>,
) {
    let start = Instant::now();
    for (thinker_ent, Actor(actor), mut thinker) in thinker_q.iter_mut().skip(iterations.index) {
        iterations.index += 1;

        let thinker_state = action_states
            .get_mut(thinker_ent)
            .expect("Where is it?")
            .clone();

        let thinker_span = thinker.span.clone();
        let _thinker_span_guard = thinker_span.enter();

        match thinker_state {
            ActionState::Init => {
                let mut act_state = action_states.get_mut(thinker_ent).expect("???");
                debug!("Initializing thinker.");
                *act_state = ActionState::Requested;
            }
            ActionState::Requested => {
                let mut act_state = action_states.get_mut(thinker_ent).expect("???");
                debug!("Thinker requested. Starting execution.");
                *act_state = ActionState::Executing;
            }
            ActionState::Success | ActionState::Failure => {}
            ActionState::Cancelled => {
                debug!("Thinker cancelled. Cleaning up.");
                if let Some((current_action, current_builder)) = thinker.current_action.take() {
                    let action_span = action_spans.get(current_action.0).expect("Where is it?");
                    debug!("Cancelling current action because thinker was cancelled.");
                    let state = action_states
                        .get_mut(current_action.0)
                        .expect("Couldn't find a component corresponding to the current action. This is definitely a bug.")
                        .clone();
                    match state {
                        ActionState::Success | ActionState::Failure => {
                            debug!("Action already wrapped up on its own. Cleaning up action in Thinker.");
                            record_completed_action_tick(&mut thinker, &current_builder);
                            if let Ok(mut ent) = cmd.get_entity(current_action.0) {
                                ent.despawn();
                            }
                        }
                        ActionState::Cancelled => {
                            debug!("Current action already cancelled.");
                            thinker.current_action = Some((current_action, current_builder));
                        }
                        _ => {
                            let mut state = action_states
                                .get_mut(current_action.0)
                                .expect("Couldn't find a component corresponding to the current action. This is definitely a bug.");
                            debug!( "Action is still executing. Attempting to cancel it before wrapping up Thinker cancellation.");
                            action_span.span.in_scope(|| {
                                debug!("Parent thinker was cancelled. Cancelling action.");
                            });
                            *state = ActionState::Cancelled;
                            thinker.current_action = Some((current_action, current_builder));
                        }
                    }
                } else {
                    let mut act_state = action_states.get_mut(thinker_ent).expect("???");
                    debug!("No current thinker action. Wrapping up Thinker as Succeeded.");
                    *act_state = ActionState::Success;
                }
            }
            ActionState::Executing => {
                thinker.decision_tick = thinker.decision_tick.saturating_add(1);
                #[cfg(feature = "trace")]
                trace!("Thinker is executing. Thinking...");
                let candidate_choices: Vec<Choice> = thinker
                    .choices
                    .iter()
                    .filter(|choice| !is_action_on_cooldown(&thinker, &choice.action))
                    .cloned()
                    .collect();
                if let Some(choice) = thinker.picker.pick(&candidate_choices, &scores) {
                    // Think about what action we're supposed to be taking. We do this
                    // every tick, because we might change our mind.
                    // ...and then execute it (details below).
                    #[cfg(feature = "trace")]
                    trace!("Action picked. Executing picked action.");
                    let action = choice.action.clone();
                    let scorer = choice.scorer;
                    let score = scores.get(choice.scorer.0).expect("Where is it?");
                    if should_keep_current_action_for_hysteresis(
                        &thinker,
                        &action,
                        score.get(),
                        &scores,
                        &mut action_states,
                    ) {
                        #[cfg(feature = "trace")]
                        trace!(
                            "Hysteresis retained current action (new score {:.3} within {:.3}).",
                            score.get(),
                            HYSTERESIS_EPSILON
                        );
                        continue;
                    }
                    exec_picked_action(
                        &mut cmd,
                        *actor,
                        &mut thinker,
                        &action,
                        &mut action_states,
                        &action_spans,
                        Some((&scorer, score)),
                        &scorer_spans,
                        true,
                    );
                } else if should_schedule_action(&mut thinker, &mut action_states) {
                    debug!("Spawning scheduled action.");
                    let action = thinker
                        .scheduled_actions
                        .pop_front()
                        .expect("we literally just checked if it was there.");
                    let new_action = actions::spawn_action(action.1.as_ref(), &mut cmd, *actor);
                    thinker.current_action = Some((Action(new_action), action.clone()));
                    thinker.current_action_label = Some(action.1.label().map(|s| s.into()));
                } else if let Some(default_action_ent) = &thinker.otherwise {
                    // Otherwise, let's just execute the default one! (if it's there)
                    let default_action_ent = default_action_ent.clone();
                    exec_picked_action(
                        &mut cmd,
                        *actor,
                        &mut thinker,
                        &default_action_ent,
                        &mut action_states,
                        &action_spans,
                        None,
                        &scorer_spans,
                        false,
                    );
                } else if let Some((action_ent, action_builder)) = thinker
                    .current_action
                    .as_ref()
                    .map(|(action, builder)| (action.0, builder.clone()))
                {
                    let action_span = action_spans.get(action_ent).expect("Where is it?");
                    let _guard = action_span.span.enter();
                    let mut curr_action_state = action_states.get_mut(action_ent).expect("Couldn't find a component corresponding to the current action. This is definitely a bug.");
                    let previous_done = matches!(
                        *curr_action_state,
                        ActionState::Success | ActionState::Failure
                    );
                    if previous_done {
                        debug!(
                            "Action completed and nothing was picked. Despawning action entity.",
                        );
                        record_completed_action_tick(&mut thinker, &action_builder);
                        // Despawn the action itself.
                        if let Ok(mut ent) = cmd.get_entity(action_ent) {
                            ent.despawn();
                        }
                        thinker.current_action = None;
                    } else if *curr_action_state == ActionState::Init {
                        *curr_action_state = ActionState::Requested;
                    }
                }
            }
        }
        if iterations.index % 500 == 0 && start.elapsed() > iterations.max_duration {
            return;
        }
    }
    iterations.index = 0;
}

fn should_schedule_action(
    thinker: &mut Mut<Thinker>,
    states: &mut Query<&mut ActionState>,
) -> bool {
    #[cfg(feature = "trace")]
    let thinker_span = thinker.span.clone();
    #[cfg(feature = "trace")]
    let _thinker_span_guard = thinker_span.enter();
    if thinker.scheduled_actions.is_empty() {
        #[cfg(feature = "trace")]
        trace!("No scheduled actions. Not scheduling anything.");
        false
    } else if let Some((action_ent, _)) = &mut thinker.current_action {
        let curr_action_state = states.get_mut(action_ent.0).expect("Couldn't find a component corresponding to the current action. This is definitely a bug.");

        let action_done = matches!(
            *curr_action_state,
            ActionState::Success | ActionState::Failure
        );

        #[cfg(feature = "trace")]
        if action_done {
            trace!("Current action is already done. Can schedule.");
        } else {
            trace!("Current action is still executing. Not scheduling anything.");
        }

        action_done
    } else {
        #[cfg(feature = "trace")]
        trace!("No current action actions. Can schedule.");
        true
    }
}

fn should_keep_current_action_for_hysteresis(
    thinker: &Thinker,
    picked_action: &ActionBuilderWrapper,
    picked_score: f32,
    scores: &Query<&Score>,
    states: &mut Query<&mut ActionState>,
) -> bool {
    let Some((current_action_ent, current_action)) = thinker.current_action.as_ref() else {
        return false;
    };

    if Arc::ptr_eq(&current_action.0, &picked_action.0) {
        return false;
    }

    let Ok(current_state) = states.get_mut(current_action_ent.entity()) else {
        return false;
    };

    if !matches!(*current_state, ActionState::Requested | ActionState::Executing) {
        return false;
    }

    let Some(current_choice_score) = thinker
        .choices
        .iter()
        .find(|choice| Arc::ptr_eq(&choice.action.0, &current_action.0))
        .map(|choice| choice.calculate(scores))
    else {
        return false;
    };

    picked_score <= current_choice_score + HYSTERESIS_EPSILON
}

fn action_key(action: &ActionBuilderWrapper) -> usize {
    Arc::as_ptr(&action.0) as usize
}

fn is_action_on_cooldown(thinker: &Thinker, action: &ActionBuilderWrapper) -> bool {
    if thinker.repeat_action_cooldown_ticks == 0 {
        return false;
    }

    let key = action_key(action);
    let Some(last_completed_tick) = thinker.completed_action_tick.get(&key) else {
        return false;
    };

    thinker.decision_tick.saturating_sub(*last_completed_tick) < thinker.repeat_action_cooldown_ticks
}

fn record_completed_action_tick(thinker: &mut Thinker, action: &ActionBuilderWrapper) {
    if thinker.repeat_action_cooldown_ticks == 0 {
        return;
    }
    thinker
        .completed_action_tick
        .insert(action_key(action), thinker.decision_tick);
}

#[allow(clippy::too_many_arguments)]
fn exec_picked_action(
    cmd: &mut Commands,
    actor: Entity,
    thinker: &mut Mut<Thinker>,
    picked_action: &ActionBuilderWrapper,
    states: &mut Query<&mut ActionState>,
    action_spans: &Query<&ActionSpan>,
    scorer_info: Option<(&Scorer, &Score)>,
    scorer_spans: &Query<&ScorerSpan>,
    override_current: bool,
) {
    // If we do find one, then we need to grab the corresponding
    // component for it. The "action" that `picker.pick()` returns
    // is just a newtype for an Entity.
    //

    // Hysteresis is handled at pick time in thinker_system. This function applies the chosen transition.
    let thinker_span = thinker.span.clone();
    let _thinker_span_guard = thinker_span.enter();
    if let Some((action_ent, current_builder)) = thinker.current_action.take() {
        let current_id = &current_builder.0;
        let mut curr_action_state = states.get_mut(action_ent.0).expect("Couldn't find a component corresponding to the current action. This is definitely a bug.");
        let previous_done = matches!(
            *curr_action_state,
            ActionState::Success | ActionState::Failure
        );
        let action_span = action_spans.get(action_ent.0).expect("Where is it?");
        let _guard = action_span.span.enter();
        if (!Arc::ptr_eq(current_id, &picked_action.0) && override_current) || previous_done {
            // So we've picked a different action than we were
            // currently executing. Just like before, we grab the
            // actual Action component (and we assume it exists).
            // If the action is executing, or was requested, we
            // need to cancel it to make sure it stops.
            if !previous_done {
                if override_current {
                    #[cfg(feature = "trace")]
                    trace!("Falling back to `otherwise` clause.",);
                } else {
                    #[cfg(feature = "trace")]
                    trace!("Picked a different action than the current one.",);
                }
            }
            match *curr_action_state {
                ActionState::Executing | ActionState::Requested => {
                    debug!("Previous action is still executing. Requesting action cancellation.",);
                    *curr_action_state = ActionState::Cancelled;
                    thinker.current_action = Some((action_ent, current_builder));
                }
                ActionState::Init | ActionState::Success | ActionState::Failure => {
                    debug!("Previous action already completed. Despawning action entity.",);
                    record_completed_action_tick(thinker, &current_builder);
                    // Despawn the action itself.
                    if let Ok(mut ent) = cmd.get_entity(action_ent.0) {
                        ent.despawn();
                    }
                    if let Some((Scorer(ent), score)) = scorer_info {
                        let scorer_span = scorer_spans.get(*ent).expect("Where is it?");
                        let _guard = scorer_span.span.enter();
                        debug!("Winning scorer chosen with score {}", score.get());
                    }
                    std::mem::drop(_guard);
                    debug!("Spawning next action");
                    let new_action =
                        Action(actions::spawn_action(picked_action.1.as_ref(), cmd, actor));
                    thinker.current_action = Some((new_action, picked_action.clone()));
                    thinker.current_action_label = Some(picked_action.1.label().map(|s| s.into()));
                }
                ActionState::Cancelled => {
                    #[cfg(feature = "trace")]
                    trace!(
                    "Cancellation already requested. Waiting for action to be marked as completed.",
                );
                    thinker.current_action = Some((action_ent, current_builder));
                }
            };
        } else {
            // Otherwise, it turns out we want to keep executing
            // the same action. Just in case, we go ahead and set
            // it as Requested if for some reason it had finished
            // but the Action System hasn't gotten around to
            // cleaning it up.
            if *curr_action_state == ActionState::Init {
                *curr_action_state = ActionState::Requested;
            }
            thinker.current_action = Some((action_ent, current_builder));
            #[cfg(feature = "trace")]
            trace!("Continuing execution of current action.",)
        }
    } else {
        #[cfg(feature = "trace")]
        trace!("Falling back to `otherwise` clause.",);

        // This branch arm is called when there's no
        // current_action in the thinker. The logic here is pretty
        // straightforward -- we set the action, Request it, and
        // that's it.
        if let Some((Scorer(ent), score)) = scorer_info {
            let scorer_span = scorer_spans.get(*ent).expect("Where is it?");
            let _guard = scorer_span.span.enter();
            debug!("Winning scorer chosen with score {}", score.get());
        }
        debug!("No current action. Spawning new action.");
        let new_action = actions::spawn_action(picked_action.1.as_ref(), cmd, actor);
        thinker.current_action = Some((Action(new_action), picked_action.clone()));
        thinker.current_action_label = Some(picked_action.1.label().map(|s| s.into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pickers::Highest;

    #[derive(Debug)]
    struct DummyAction;

    impl ActionBuilder for DummyAction {
        fn build(&self, _cmd: &mut Commands, _action: Entity, _actor: Entity) {}
    }

    fn test_thinker(cooldown_ticks: u64) -> Thinker {
        Thinker {
            picker: Arc::new(Highest),
            otherwise: None,
            choices: Vec::new(),
            current_action: None,
            current_action_label: None,
            span: span!(Level::DEBUG, "test_thinker"),
            scheduled_actions: VecDeque::new(),
            decision_tick: 0,
            repeat_action_cooldown_ticks: cooldown_ticks,
            completed_action_tick: HashMap::new(),
        }
    }

    #[test]
    fn repeat_action_cooldown_blocks_recently_completed_action() {
        let action = ActionBuilderWrapper::new(Arc::new(DummyAction));
        let mut thinker = test_thinker(3);

        thinker.decision_tick = 10;
        record_completed_action_tick(&mut thinker, &action);

        thinker.decision_tick = 12;
        assert!(is_action_on_cooldown(&thinker, &action));

        thinker.decision_tick = 13;
        assert!(!is_action_on_cooldown(&thinker, &action));
    }

    #[test]
    fn repeat_action_cooldown_zero_disables_cooldown() {
        let action = ActionBuilderWrapper::new(Arc::new(DummyAction));
        let mut thinker = test_thinker(0);

        thinker.decision_tick = 5;
        record_completed_action_tick(&mut thinker, &action);
        thinker.decision_tick = 6;

        assert!(!is_action_on_cooldown(&thinker, &action));
    }
}
