/// Why something needs the user, most urgent first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttentionKind {
    DeadlinePassed,
    ClosingSoon,
    NotSent,
    FollowUpDue,
    Stale,
    GuardFlag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionItem {
    pub kind: AttentionKind,
    /// Days left or days quiet, used to order items of one kind.
    pub urgency: i64,
    pub title: String,
    pub why: String,
    pub command: String,
}

pub fn most_urgent_first(items: &mut [AttentionItem]) {
    items.sort_by_key(|item| (item.kind, item.urgency));
}
