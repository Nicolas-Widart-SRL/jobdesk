use std::time::{Duration, Instant};

use crate::runs::domain::model_family;

/// How long a run can go without a word from Claude before it shows as quiet.
pub const QUIET_AFTER: Duration = Duration::from_secs(30);
/// How long before the quiet turns into a long one.
pub const LONG_QUIET_AFTER: Duration = Duration::from_secs(120);

/// How long since the last word from Claude, ranked. Each holds the time itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quiet {
    Fresh(Duration),
    Quiet(Duration),
    Long(Duration),
}

pub fn quiet(since_update: Duration) -> Quiet {
    if since_update < QUIET_AFTER {
        Quiet::Fresh(since_update)
    } else if since_update < LONG_QUIET_AFTER {
        Quiet::Quiet(since_update)
    } else {
        Quiet::Long(since_update)
    }
}

/// How long a run has worked, by the clock. It never reads the clock: every `now` comes from the caller.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Elapsed {
    /// Time of the spans already closed.
    held: Duration,
    /// When the open span began, while one is open.
    since: Option<Instant>,
}

impl Elapsed {
    /// A run read back from its log: its time is what Claude reported, and no span is open.
    pub fn restored(ms: u64) -> Self {
        Self { held: Duration::from_millis(ms), since: None }
    }

    /// Opens a span. One already open keeps its start.
    pub fn start(&mut self, now: Instant) {
        self.since.get_or_insert(now);
    }

    /// Closes the open span and adds its time.
    pub fn hold(&mut self, now: Instant) {
        if let Some(since) = self.since.take() {
            self.held += now.saturating_duration_since(since);
        }
    }

    pub fn is_running(&self) -> bool {
        self.since.is_some()
    }

    pub fn total(&self, now: Instant) -> Duration {
        self.held + self.since.map_or(Duration::ZERO, |since| now.saturating_duration_since(since))
    }
}

/// Tokens one message used, as the CLI counts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
}

/// US dollars per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    pub cache_write: f64,
    pub cache_read: f64,
}

/// The list price of a model's family, one price per family whatever its version. A model of no known family, or none named,
/// is priced as Opus. So the estimate is approximate either way: an older Opus priced higher reads low, and a cheaper unknown
/// model reads high. The CLI's own figure replaces it when the turn ends.
pub fn price_for(model: &str) -> Price {
    match model_family(model).as_str() {
        "Sonnet" => Price { input: 3.0, output: 15.0, cache_write: 3.75, cache_read: 0.30 },
        "Haiku" => Price { input: 1.0, output: 5.0, cache_write: 1.25, cache_read: 0.10 },
        _ => Price { input: 5.0, output: 25.0, cache_write: 6.25, cache_read: 0.50 },
    }
}

/// What the tokens cost at `model`'s list price, before the CLI reports the real figure.
pub fn estimate_usd(tokens: &Tokens, model: &str) -> f64 {
    let price = price_for(model);
    let cost = |count: u64, per_million: f64| count as f64 * per_million / 1_000_000.0;
    cost(tokens.input, price.input)
        + cost(tokens.output, price.output)
        + cost(tokens.cache_write, price.cache_write)
        + cost(tokens.cache_read, price.cache_read)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn quiet_has_three_steps() {
        assert_eq!(quiet(secs(29)), Quiet::Fresh(secs(29)));
        assert_eq!(quiet(secs(30)), Quiet::Quiet(secs(30)));
        assert_eq!(quiet(secs(119)), Quiet::Quiet(secs(119)));
        assert_eq!(quiet(secs(120)), Quiet::Long(secs(120)));
    }

    #[test]
    fn elapsed_adds_its_spans() {
        let t0 = Instant::now();
        let mut elapsed = Elapsed::default();
        assert!(!elapsed.is_running());
        elapsed.start(t0);
        elapsed.start(t0 + secs(5));
        assert_eq!(elapsed.total(t0 + secs(10)), secs(10), "a second start keeps the first");
        elapsed.hold(t0 + secs(10));
        assert!(!elapsed.is_running());
        assert_eq!(elapsed.total(t0 + secs(50)), secs(10), "held time stands still");
        elapsed.hold(t0 + secs(60));
        assert_eq!(elapsed.total(t0 + secs(60)), secs(10), "holding twice adds nothing");
        elapsed.start(t0 + secs(100));
        assert_eq!(elapsed.total(t0 + secs(104)), secs(14));
    }

    #[test]
    fn a_restored_run_starts_from_what_claude_reported() {
        let t0 = Instant::now();
        let mut elapsed = Elapsed::restored(860_000);
        assert_eq!(elapsed.total(t0), Duration::from_millis(860_000));
        elapsed.start(t0);
        assert_eq!(elapsed.total(t0 + secs(5)), Duration::from_millis(865_000));
    }

    #[test]
    fn a_family_sets_the_price() {
        let tokens = Tokens { input: 1_000_000, output: 1_000_000, cache_write: 1_000_000, cache_read: 1_000_000 };
        assert_eq!(estimate_usd(&tokens, "claude-opus-4-5"), 36.75);
        assert_eq!(estimate_usd(&tokens, "claude-sonnet-4-5-20250929"), 22.05);
        assert_eq!(estimate_usd(&tokens, "claude-haiku-4-5"), 7.35);
        assert_eq!(estimate_usd(&tokens, "acme-model-9"), 36.75, "an unknown model is priced as Opus");
    }

    #[test]
    fn no_tokens_cost_nothing() {
        assert_eq!(estimate_usd(&Tokens::default(), "claude-opus-4-5"), 0.0);
    }
}
