use crate::{DiscoveredContactRoute, PlatformContactObservation};

pub(crate) fn discovered_observation(
    observation: PlatformContactObservation,
) -> DiscoveredContactRoute {
    DiscoveredContactRoute {
        channel: observation.channel,
        kind: observation.kind,
        value: observation.value,
        source: observation.source,
        source_url: observation.source_url,
    }
}
