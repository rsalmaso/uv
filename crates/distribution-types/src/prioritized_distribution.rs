use pep440_rs::VersionSpecifiers;
use platform_tags::{IncompatibleTag, TagCompatibility, TagPriority};
use pypi_types::{Hashes, Yanked};

use crate::Dist;

/// A collection of distributions that have been filtered by relevance.
#[derive(Debug, Default, Clone)]
pub struct PrioritizedDist(Box<PrioritizedDistInner>);

/// [`PrioritizedDist`] is boxed because [`Dist`] is large.
#[derive(Debug, Default, Clone)]
struct PrioritizedDistInner {
    /// An arbitrary, compatible source distribution for the package version.
    compatible_source: Option<Dist>,
    /// The highest-priority, installable wheel for the package version.
    compatible_wheel: Option<(Dist, TagPriority)>,
    /// The most-relevant, incompatible wheel for the package version.
    incompatible_wheel: Option<(Dist, IncompatibleWheel)>,
    /// An arbitrary, incompatible source distribution for the package version.
    incompatible_source: Option<(Dist, IncompatibleSource)>,
    /// The hashes for each distribution.
    hashes: Vec<Hashes>,
}

/// A distribution that can be used for both resolution and installation.
#[derive(Debug, Clone)]
pub enum CompatibleDist<'a> {
    /// The distribution should be resolved and installed using a source distribution.
    SourceDist(&'a Dist),
    /// The distribution should be resolved and installed using a wheel distribution.
    CompatibleWheel(&'a Dist, TagPriority),
    /// The distribution should be resolved using an incompatible wheel distribution, but
    /// installed using a source distribution.
    IncompatibleWheel {
        source_dist: &'a Dist,
        wheel: &'a Dist,
    },
}

#[derive(Debug, Clone)]
pub enum IncompatibleDist {
    /// An incompatible wheel is available.
    Wheel(IncompatibleWheel),
    /// An incompatible source distribution is available.
    Source(IncompatibleSource),
    /// No distributions are available
    Unavailable,
}

#[derive(Debug, PartialEq, Eq)]
pub enum WheelCompatibility {
    Incompatible(IncompatibleWheel),
    Compatible(TagPriority),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum IncompatibleWheel {
    ExcludeNewer(Option<i64>),
    Tag(IncompatibleTag),
    RequiresPython(VersionSpecifiers),
    Yanked(Yanked),
    NoBinary,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SourceDistCompatibility {
    Incompatible(IncompatibleSource),
    Compatible,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum IncompatibleSource {
    ExcludeNewer(Option<i64>),
    RequiresPython(VersionSpecifiers),
    Yanked(Yanked),
    NoBuild,
}

impl PrioritizedDist {
    /// Create a new [`PrioritizedDist`] from the given wheel distribution.
    pub fn from_built(dist: Dist, hash: Option<Hashes>, compatibility: WheelCompatibility) -> Self {
        match compatibility {
            WheelCompatibility::Compatible(priority) => Self(Box::new(PrioritizedDistInner {
                compatible_source: None,
                compatible_wheel: Some((dist, priority)),
                incompatible_wheel: None,
                incompatible_source: None,
                hashes: hash.map(|hash| vec![hash]).unwrap_or_default(),
            })),
            WheelCompatibility::Incompatible(incompatibility) => {
                Self(Box::new(PrioritizedDistInner {
                    compatible_source: None,
                    compatible_wheel: None,
                    incompatible_wheel: Some((dist, incompatibility)),
                    incompatible_source: None,
                    hashes: hash.map(|hash| vec![hash]).unwrap_or_default(),
                }))
            }
        }
    }

    /// Create a new [`PrioritizedDist`] from the given source distribution.
    pub fn from_source(
        dist: Dist,
        hash: Option<Hashes>,
        compatibility: SourceDistCompatibility,
    ) -> Self {
        match compatibility {
            SourceDistCompatibility::Compatible => Self(Box::new(PrioritizedDistInner {
                compatible_source: Some(dist),
                incompatible_source: None,
                compatible_wheel: None,
                incompatible_wheel: None,
                hashes: hash.map(|hash| vec![hash]).unwrap_or_default(),
            })),
            SourceDistCompatibility::Incompatible(incompatibility) => {
                Self(Box::new(PrioritizedDistInner {
                    compatible_source: None,
                    incompatible_source: Some((dist, incompatibility)),
                    compatible_wheel: None,
                    incompatible_wheel: None,
                    hashes: hash.map(|hash| vec![hash]).unwrap_or_default(),
                }))
            }
        }
    }

    /// Insert the given built distribution into the [`PrioritizedDist`].
    pub fn insert_built(
        &mut self,
        dist: Dist,
        hash: Option<Hashes>,
        compatibility: WheelCompatibility,
    ) {
        match compatibility {
            // Prefer the highest-priority, compatible wheel.
            WheelCompatibility::Compatible(priority) => {
                if let Some((.., existing_priority)) = &self.0.compatible_wheel {
                    if priority > *existing_priority {
                        self.0.compatible_wheel = Some((dist, priority));
                    }
                } else {
                    self.0.compatible_wheel = Some((dist, priority));
                }
            }
            // Track the most relevant incompatible wheel
            WheelCompatibility::Incompatible(incompatibility) => {
                if let Some((.., existing_incompatibility)) = &self.0.incompatible_wheel {
                    if incompatibility.is_more_compatible(existing_incompatibility) {
                        self.0.incompatible_wheel = Some((dist, incompatibility));
                    }
                } else {
                    self.0.incompatible_wheel = Some((dist, incompatibility));
                }
            }
        }

        if let Some(hash) = hash {
            self.0.hashes.push(hash);
        }
    }

    /// Insert the given source distribution into the [`PrioritizedDist`].
    pub fn insert_source(
        &mut self,
        dist: Dist,
        hash: Option<Hashes>,
        compatibility: SourceDistCompatibility,
    ) {
        match compatibility {
            SourceDistCompatibility::Compatible => {
                if self.0.compatible_source.is_none() {
                    self.0.compatible_source = Some(dist);
                }
            }
            SourceDistCompatibility::Incompatible(incompatibility) => {
                if let Some((.., existing_incompatibility)) = &self.0.incompatible_source {
                    if incompatibility.is_more_compatible(existing_incompatibility) {
                        self.0.incompatible_source = Some((dist, incompatibility));
                    }
                } else {
                    self.0.incompatible_source = Some((dist, incompatibility));
                }
            }
        }

        if let Some(hash) = hash {
            self.0.hashes.push(hash);
        }
    }

    /// Return the highest-priority distribution for the package version, if any.
    pub fn get(&self) -> Option<CompatibleDist> {
        match (
            &self.0.compatible_wheel,
            &self.0.compatible_source,
            &self.0.incompatible_wheel,
        ) {
            // Prefer the highest-priority, platform-compatible wheel.
            (Some((wheel, tag_priority)), _, _) => {
                Some(CompatibleDist::CompatibleWheel(wheel, *tag_priority))
            }
            // If we have a compatible source distribution and an incompatible wheel, return the
            // wheel. We assume that all distributions have the same metadata for a given package
            // version. If a compatible source distribution exists, we assume we can build it, but
            // using the wheel is faster.
            (_, Some(source_dist), Some((wheel, _))) => {
                Some(CompatibleDist::IncompatibleWheel { source_dist, wheel })
            }
            // Otherwise, if we have a source distribution, return it.
            (_, Some(source_dist), _) => Some(CompatibleDist::SourceDist(source_dist)),
            _ => None,
        }
    }

    /// Return the source distribution, if any.
    pub fn compatible_source(&self) -> Option<&Dist> {
        self.0.compatible_source.as_ref()
    }

    /// Return the incompatible source distribution, if any.
    pub fn incompatible_source(&self) -> Option<&(Dist, IncompatibleSource)> {
        self.0.incompatible_source.as_ref()
    }

    /// Return the compatible built distribution, if any.
    pub fn compatible_wheel(&self) -> Option<&(Dist, TagPriority)> {
        self.0.compatible_wheel.as_ref()
    }

    /// Return the incompatible built distribution, if any.
    pub fn incompatible_wheel(&self) -> Option<&(Dist, IncompatibleWheel)> {
        self.0.incompatible_wheel.as_ref()
    }

    /// Return the hashes for each distribution.
    pub fn hashes(&self) -> &[Hashes] {
        &self.0.hashes
    }

    /// Returns true if and only if this distribution does not contain any
    /// source distributions or wheels.
    pub fn is_empty(&self) -> bool {
        self.0.compatible_source.is_none()
            && self.0.incompatible_source.is_none()
            && self.0.compatible_wheel.is_none()
            && self.0.incompatible_wheel.is_none()
    }
}

impl<'a> CompatibleDist<'a> {
    /// Return the [`Dist`] to use during resolution.
    pub fn for_resolution(&self) -> &Dist {
        match *self {
            CompatibleDist::SourceDist(sdist) => sdist,
            CompatibleDist::CompatibleWheel(wheel, _) => wheel,
            CompatibleDist::IncompatibleWheel {
                source_dist: _,
                wheel,
            } => wheel,
        }
    }

    /// Return the [`Dist`] to use during installation.
    pub fn for_installation(&self) -> &Dist {
        match *self {
            CompatibleDist::SourceDist(sdist) => sdist,
            CompatibleDist::CompatibleWheel(wheel, _) => wheel,
            CompatibleDist::IncompatibleWheel {
                source_dist,
                wheel: _,
            } => source_dist,
        }
    }
}

impl WheelCompatibility {
    pub fn is_compatible(&self) -> bool {
        matches!(self, Self::Compatible(_))
    }
}

impl From<TagCompatibility> for WheelCompatibility {
    fn from(value: TagCompatibility) -> Self {
        match value {
            TagCompatibility::Compatible(priority) => WheelCompatibility::Compatible(priority),
            TagCompatibility::Incompatible(tag) => {
                WheelCompatibility::Incompatible(IncompatibleWheel::Tag(tag))
            }
        }
    }
}

impl IncompatibleSource {
    fn is_more_compatible(&self, other: &IncompatibleSource) -> bool {
        match self {
            Self::ExcludeNewer(timestamp_self) => match other {
                // Smaller timestamps are closer to the cut-off time
                Self::ExcludeNewer(timestamp_other) => timestamp_other < timestamp_self,
                Self::NoBuild | Self::RequiresPython(_) | Self::Yanked(_) => true,
            },
            Self::RequiresPython(_) => match other {
                Self::ExcludeNewer(_) => false,
                // Version specifiers cannot be reasonably compared
                Self::RequiresPython(_) => false,
                Self::NoBuild | Self::Yanked(_) => true,
            },
            Self::Yanked(_) => match other {
                Self::ExcludeNewer(_) | Self::RequiresPython(_) => false,
                // Yanks with a reason are more helpful for errors
                Self::Yanked(yanked_other) => matches!(yanked_other, Yanked::Reason(_)),
                Self::NoBuild => true,
            },
            Self::NoBuild => false,
        }
    }
}

impl IncompatibleWheel {
    fn is_more_compatible(&self, other: &IncompatibleWheel) -> bool {
        match self {
            Self::ExcludeNewer(timestamp_self) => match other {
                // Smaller timestamps are closer to the cut-off time
                Self::ExcludeNewer(timestamp_other) => match (timestamp_self, timestamp_other) {
                    (None, _) => true,
                    (_, None) => false,
                    (Some(timestamp_self), Some(timestamp_other)) => {
                        timestamp_other < timestamp_self
                    }
                },
                Self::NoBinary | Self::RequiresPython(_) | Self::Tag(_) | Self::Yanked(_) => true,
            },
            Self::Tag(tag_self) => match other {
                Self::ExcludeNewer(_) => false,
                Self::Tag(tag_other) => tag_other > tag_self,
                Self::NoBinary | Self::RequiresPython(_) | Self::Yanked(_) => true,
            },
            Self::RequiresPython(_) => match other {
                Self::ExcludeNewer(_) | Self::Tag(_) => false,
                // Version specifiers cannot be reasonably compared
                Self::RequiresPython(_) => false,
                Self::NoBinary | Self::Yanked(_) => true,
            },
            Self::Yanked(_) => match other {
                Self::ExcludeNewer(_) | Self::Tag(_) | Self::RequiresPython(_) => false,
                // Yanks with a reason are more helpful for errors
                Self::Yanked(yanked_other) => matches!(yanked_other, Yanked::Reason(_)),
                Self::NoBinary => true,
            },
            Self::NoBinary => false,
        }
    }
}
