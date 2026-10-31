use crate::insights::domain::{ApplicationFact, Furthest, Progress, SearchReport};
use crate::tracking::application::{ApplicationCard, Pipeline, ReachedStage, StatusBucket};

pub type SearchInsights = SearchReport;

/// Translates tracking's model into this context's facts.
pub fn search_insights(pipeline: &Pipeline) -> SearchInsights {
    let facts: Vec<_> = pipeline.cards().map(fact_from).collect();
    SearchReport::from_facts(&facts)
}

fn fact_from(card: &ApplicationCard) -> ApplicationFact {
    ApplicationFact {
        progress: match card.bucket {
            StatusBucket::Drafted => Progress::Drafted,
            StatusBucket::Active => Progress::Active,
            StatusBucket::Interview => Progress::Interview,
            StatusBucket::Offer => Progress::Offer,
            StatusBucket::Hired => Progress::Hired,
            StatusBucket::Closed => Progress::Closed,
        },
        furthest: match card.reached {
            ReachedStage::Drafted | ReachedStage::Applied => Furthest::Applied,
            ReachedStage::Interview => Furthest::Interview,
            ReachedStage::Offer => Furthest::Offer,
            ReachedStage::Hired => Furthest::Hired,
        },
        sector: card.sector.clone(),
        channel: card.channel.clone(),
        is_rejection: card.is_rejection,
        is_final: card.is_final,
    }
}
