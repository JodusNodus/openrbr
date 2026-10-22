//! Named callees selected by the NGP adapter. No executable instructions are decoded for dispatch.

/// Functions NGP replaces, and signal wrappers whose host preserves the original fan-out.
#[derive(Clone, Copy)]
pub(crate) enum Call {
    DriveLineSample,
    BodyEvent,
    BrakeWall,
    CarProblem,
    EngineNoise,
    Exhaust,
    TyreNoise,
    Retire,
    StandStill,
    DamperSignal,
    SpringStep,
    TyreStep,
    TyreParticles,
}

impl Call {
    pub(crate) fn address(self) -> u32 {
        let (original, ngp) = match self {
            Self::DriveLineSample => (0x50b300, 0x10015fc0),
            Self::BodyEvent => (0x52be70, 0x100edbf0),
            Self::BrakeWall => (0x4e4490, 0x100ed470),
            Self::CarProblem => (0x4e8a70, 0x100ed8a0),
            Self::EngineNoise => (0x50ef50, 0x100ee000),
            Self::Exhaust => (0x50ee90, 0x100ee0f0),
            Self::TyreNoise => (0x534f80, 0x100ee970),
            Self::Retire => (0x4e8770, 0x100ee6e0),
            Self::StandStill => (0x4e88f0, 0x100eda50),
            Self::DamperSignal => (0x52de60, 0x100edb20),
            Self::SpringStep => (0x52e090, 0x10059f70),
            Self::TyreStep => (0x5360c0, 0x1005b660),
            Self::TyreParticles => (0x53d0d0, 0x1005f8a0),
        };
        super::at(ngp).unwrap_or(original)
    }
}
