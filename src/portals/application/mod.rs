mod view;

pub use view::{Badge, PortalSummary, PortalView, PortalsView, portals_view};

pub use crate::portals::domain::{BUILT_IN, Origin, Phrase, Priority, Quotes, SearchPlan, Term, site_lines_for, term_as_sent};
