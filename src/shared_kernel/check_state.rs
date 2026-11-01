/// How one check on a posting came out. Only the safety check can be unchecked: the ad never went through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Pass,
    Unclear,
    Fail,
    Unchecked,
}
