//! Review identity, monotonic updates, and cleanup. No approval RPCs live here.

use super::{ConversationActivity, ConversationPhase, ConversationState};
use crate::agent::{
    AgentAutoApprovalReview, AgentAutoApprovalReviewStatus, AgentEvent, AgentGuardianWarning,
    AgentStrictReviewRequirement,
};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AutoApprovalReviewPresentation {
    pub review: AgentAutoApprovalReview,
    /// Turn cleanup is local evidence, never an invented server decision/time.
    pub closed_locally: bool,
    pub attached_to_item: bool,
}

impl AutoApprovalReviewPresentation {
    pub fn status(&self) -> AgentAutoApprovalReviewStatus {
        if self.closed_locally && self.review.status == AgentAutoApprovalReviewStatus::InProgress {
            AgentAutoApprovalReviewStatus::Aborted
        } else {
            self.review.status
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StrictReviewPresentation {
    pub requirement: AgentStrictReviewRequirement,
    pub turn_finished: bool,
}

fn terminal(phase: ConversationPhase) -> bool {
    matches!(
        phase,
        ConversationPhase::Complete | ConversationPhase::Stopped | ConversationPhase::Failed
    )
}

fn upsert(
    activities: &mut Vec<ConversationActivity>,
    review: AgentAutoApprovalReview,
    finished: bool,
) {
    if let Some(ConversationActivity::AutoApprovalReview(existing)) = activities.iter_mut().find(|a|
        matches!(a, ConversationActivity::AutoApprovalReview(existing) if existing.review.key == review.key)
    ) {
        // A repeated/late start cannot roll a completed review back to inProgress.
        // Completed messages carry the full action and server timestamps.
        if existing.review.completed_at_ms.is_some() && review.completed_at_ms.is_none() { return; }
        if existing.review.status != AgentAutoApprovalReviewStatus::InProgress
            && review.status == AgentAutoApprovalReviewStatus::InProgress { return; }
        if let (Some(old), Some(new)) = (existing.review.completed_at_ms, review.completed_at_ms)
            && new < old { return; }
        let mut review = review;
        if review.rationale.is_none() { review.rationale.clone_from(&existing.review.rationale); }
        if review.risk_level.is_none() { review.risk_level.clone_from(&existing.review.risk_level); }
        if review.user_authorization.is_none() { review.user_authorization.clone_from(&existing.review.user_authorization); }
        existing.review = review;
        existing.closed_locally |= finished;
    } else {
        activities.push(ConversationActivity::AutoApprovalReview(Box::new(AutoApprovalReviewPresentation { review, closed_locally: finished, attached_to_item: false })));
    }
}

impl ConversationState {
    pub(crate) fn apply_auto_approval_review(&mut self, review: AgentAutoApprovalReview) {
        if self.thread_id.as_deref() != Some(review.key.thread_id.as_str()) {
            return;
        }
        if let Some(turn) = self
            .transcript
            .iter_mut()
            .find(|t| t.turn_id.as_deref() == Some(review.key.turn_id.as_str()))
        {
            upsert(&mut turn.activities, review, terminal(turn.phase));
        } else if self.turn_id.as_deref() == Some(review.key.turn_id.as_str()) {
            upsert(&mut self.activities, review, terminal(self.phase));
        } else {
            self.queue_review_event(
                review.key.turn_id.clone(),
                AgentEvent::AutoApprovalReviewUpdated(Box::new(review)),
            );
        }
    }

    pub(crate) fn apply_strict_review(&mut self, requirement: AgentStrictReviewRequirement) {
        if self.thread_id.as_deref() != Some(requirement.thread_id.as_str()) {
            return;
        }
        let (activities, finished) = if let Some(turn) = self
            .transcript
            .iter_mut()
            .find(|t| t.turn_id.as_deref() == Some(requirement.turn_id.as_str()))
        {
            (&mut turn.activities, terminal(turn.phase))
        } else if self.turn_id.as_deref() == Some(requirement.turn_id.as_str()) {
            (&mut self.activities, terminal(self.phase))
        } else {
            self.queue_review_event(
                requirement.turn_id.clone(),
                AgentEvent::StrictReviewRequired(requirement),
            );
            return;
        };
        // There is no reviewId here. Distinct server start times remain independent.
        if !activities.iter().any(|a| matches!(a, ConversationActivity::StrictReview(existing) if existing.requirement == requirement)) {
            activities.push(ConversationActivity::StrictReview(StrictReviewPresentation { requirement, turn_finished: finished }));
        }
    }

    pub(crate) fn apply_guardian_warning(&mut self, warning: AgentGuardianWarning) {
        if self.thread_id.as_deref() != Some(warning.thread_id.as_str()) {
            return;
        }
        if self.activities.iter()
            .any(|a| matches!(a, ConversationActivity::GuardianWarning(existing) if existing == &warning)) { return; }
        self.activities
            .push(ConversationActivity::GuardianWarning(warning));
    }

    pub(crate) fn close_auto_approval_reviews(&mut self) {
        for activity in &mut self.activities {
            match activity {
                ConversationActivity::AutoApprovalReview(review) => review.closed_locally = true,
                ConversationActivity::StrictReview(requirement) => requirement.turn_finished = true,
                _ => {}
            }
        }
    }

    fn queue_review_event(&mut self, turn_id: String, event: AgentEvent) {
        let pending = self.pending_review_events.entry(turn_id).or_default();
        if !pending.contains(&event) {
            pending.push(event);
        }
    }

    pub(crate) fn replay_pending_reviews(&mut self) {
        let ids = self
            .transcript
            .iter()
            .filter_map(|t| t.turn_id.clone())
            .chain(self.turn_id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            if let Some(events) = self.pending_review_events.remove(&id) {
                self.apply_agent_event_batch(events);
            }
        }
    }
}
