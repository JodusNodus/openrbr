# NGP 7.4.778 .lsp key tables

NGP's copies of the stock key tables and the stock records it renames, read from a snapshot of the game running NGP. Input of `scripts/gen-ngp-lsp-keys.py`, which writes `rust/rbr-formats/src/lsp_keys_ngp.rs`; keep the heading and table shapes. Context: [notes/ngp.md](../notes/ngp.md#ngp-lsp-keys).

## Extended tables

### CCar::CDef (dict `0x8ef73c`, NGP table `0x10171318`, 20 records: 16 stock, 4 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `reserved_NGP` | `+0x4444` | `SubTics` (renamed) |
| `ForceFeedbackSensitivity_NGP` | `+0x4494` | `MaxFFTorque` (renamed) |
| `FrontDownforce_NGP` | `+0x1f0` | new |
| `RearDownforce_NGP` | `+0x200` | new |

### CDrive::CDef (dict `0x1659134`, NGP table `0x10171480`, 69 records: 14 stock, 55 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `ShiftLatency_NGP` | `+0xa58` | `WheelInertia` (renamed) |
| `DriveTrain_NGP` | `+0x634` | shares `DriveType` |
| `BrakeMass_LF_NGP` | `+0x15c` | new |
| `BrakeConvFtr_LF_NGP` | `+0x16c` | new |
| `BrakeLayerMassPct_LF_NGP` | `+0x17c` | new |
| `BrakeRadiusEff_LF_NGP` | `+0x18c` | new |
| `BrakeSpecHeat_LF_NGP` | `+0x19c` | new |
| `BrakeHeatAbsRatio_LF_NGP` | `+0x1ac` | new |
| `BrakePlateArea_LF_NGP` | `+0x1bc` | new |
| `BrakeVentArea_LF_NGP` | `+0x1cc` | new |
| `BrakeIntConvec_LF_NGP` | `+0x1dc` | new |
| `BrakePistonArea_LF_NGP` | `+0x1ec` | new |
| `BrakeMass_RF_NGP` | `+0x24c` | new |
| `BrakeConvFtr_RF_NGP` | `+0x25c` | new |
| `BrakeLayerMassPct_RF_NGP` | `+0x26c` | new |
| `BrakeRadiusEff_RF_NGP` | `+0x27c` | new |
| `BrakeSpecHeat_RF_NGP` | `+0x28c` | new |
| `BrakeHeatAbsRatio_RF_NGP` | `+0x29c` | new |
| `BrakePlateArea_RF_NGP` | `+0x2ac` | new |
| `BrakeVentArea_RF_NGP` | `+0x2bc` | new |
| `BrakeIntConvec_RF_NGP` | `+0x2cc` | new |
| `BrakePistonArea_RF_NGP` | `+0x2dc` | new |
| `BrakeMass_LR_NGP` | `+0x33c` | new |
| `BrakeConvFtr_LR_NGP` | `+0x34c` | new |
| `BrakeLayerMassPct_LR_NGP` | `+0x35c` | new |
| `BrakeRadiusEff_LR_NGP` | `+0x36c` | new |
| `BrakeSpecHeat_LR_NGP` | `+0x37c` | new |
| `BrakeHeatAbsRatio_LR_NGP` | `+0x38c` | new |
| `BrakePlateArea_LR_NGP` | `+0x39c` | new |
| `BrakeVentArea_LR_NGP` | `+0x3ac` | new |
| `BrakeIntConvec_LR_NGP` | `+0x3bc` | new |
| `BrakePistonArea_LR_NGP` | `+0x3cc` | new |
| `BrakeMass_RR_NGP` | `+0x42c` | new |
| `BrakeConvFtr_RR_NGP` | `+0x43c` | new |
| `BrakeLayerMassPct_RR_NGP` | `+0x44c` | new |
| `BrakeRadiusEff_RR_NGP` | `+0x45c` | new |
| `BrakeSpecHeat_RR_NGP` | `+0x46c` | new |
| `BrakeHeatAbsRatio_RR_NGP` | `+0x47c` | new |
| `BrakePlateArea_RR_NGP` | `+0x48c` | new |
| `BrakeVentArea_RR_NGP` | `+0x49c` | new |
| `BrakeIntConvec_RR_NGP` | `+0x4ac` | new |
| `BrakePistonArea_RR_NGP` | `+0x4bc` | new |
| `BrakeTempThresholdLow_NGP` | `+0x4ec` | new |
| `BrakeTempThresholdHigh_NGP` | `+0x4fc` | new |
| `BrakeTempThresholdMid_NGP` | `+0x50c` | new |
| `BrakeTempWearThresholdLow_NGP` | `+0x51c` | new |
| `BrakeWearFactor_NGP` | `+0x52c` | new |
| `BrakeForceWearFactor_NGP` | `+0x53c` | new |
| `LowBrakeTempCOF_NGP` | `+0x54c` | new |
| `MidBrakeTempCOF_NGP` | `+0x55c` | new |
| `HighBrakeTempCOF_NGP` | `+0x56c` | new |
| `WornBrakeCOF_NGP` | `+0x57c` | new |
| `BrakeWearLimit_NGP` | `+0x58c` | new |
| `GearActuatorDisengageTime_NGP` | `+0xa68` | new |
| `GearActuatorEngageTime_NGP` | `+0xa78` | new |

### CDrive::CSetup (dict `0x165915c`, NGP table `0x101718d0`, 28 records: 17 stock, 11 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `HighSpeedBreakReboundFront_NGP` | `+0x4bc` | new |
| `HighSpeedDampingReboundFront_NGP` | `+0x4cc` | new |
| `HighSpeedBreakReboundRear_NGP` | `+0x4dc` | new |
| `HighSpeedDampingReboundRear_NGP` | `+0x4ec` | new |
| `BumpStopStiffnessFront_NGP` | `+0x4fc` | new |
| `BumpStopDampingBumpFront_NGP` | `+0x50c` | new |
| `BumpStopDampingReboundFront_NGP` | `+0x51c` | new |
| `BumpStopStiffnessRear_NGP` | `+0x52c` | new |
| `BumpStopDampingBumpRear_NGP` | `+0x53c` | new |
| `BumpStopDampingReboundRear_NGP` | `+0x54c` | new |
| `HandbrakePercentage_NGP` | `+0x56c` | new |

### CEngine::CDef (dict `0x16591b0`, NGP table `0x10171ae8`, 133 records: 17 stock, 116 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `EngineType_NGP` | `+0x26ec` | shares `Turbo` |
| `TurboInertia_NGP` | `+0xc` | new |
| `TurboCompressorCoefficient_NGP` | `+0x1c` | new |
| `TurboTurbineCoefficient_NGP` | `+0x2c` | new |
| `TurboMinBlowOffPressure_NGP` | `+0x3c` | new |
| `TurboMaxBlowOffPressure_NGP` | `+0x4c` | new |
| `TurboBlowOffCoefficient_NGP` | `+0x5c` | new |
| `TurboMinWasteGatePressure_NGP` | `+0x6c` | new |
| `TurboMaxWasteGatePressure_NGP` | `+0x7c` | new |
| `TurboWasteGateCoefficient_NGP` | `+0x8c` | new |
| `TurboLinearFrictionFactor_NGP` | `+0xac` | new |
| `TurboSquareFrictionFactor_NGP` | `+0xbc` | new |
| `TurboMaxOilFlowPressure_NGP` | `+0xcc` | new |
| `TurboMinOilFlowPressure_NGP` | `+0xdc` | new |
| `MaxOilGalleryFlowPressure_00_NGP` | `+0x10c` | new |
| `MinOilGalleryFlowPressure_00_NGP` | `+0x11c` | new |
| `MinOilFlowRatePercentage_00_NGP` | `+0x12c` | new |
| `MaxOilGalleryFlowPressure_01_NGP` | `+0x14c` | new |
| `MinOilGalleryFlowPressure_01_NGP` | `+0x15c` | new |
| `MinOilFlowRatePercentage_01_NGP` | `+0x16c` | new |
| `MaxOilGalleryFlowPressure_02_NGP` | `+0x18c` | new |
| `MinOilGalleryFlowPressure_02_NGP` | `+0x19c` | new |
| `MinOilFlowRatePercentage_02_NGP` | `+0x1ac` | new |
| `MaxOilGalleryFlowPressure_03_NGP` | `+0x1cc` | new |
| `MinOilGalleryFlowPressure_03_NGP` | `+0x1dc` | new |
| `MinOilFlowRatePercentage_03_NGP` | `+0x1ec` | new |
| `Tq_00_RPM_NGP` | `+0x2244` | new |
| `Tq_01_RPM_NGP` | `+0x2264` | new |
| `Tq_02_RPM_NGP` | `+0x2284` | new |
| `Tq_03_RPM_NGP` | `+0x22a4` | new |
| `Tq_04_RPM_NGP` | `+0x22c4` | new |
| `Tq_05_RPM_NGP` | `+0x22e4` | new |
| `Tq_06_RPM_NGP` | `+0x2304` | new |
| `Tq_07_RPM_NGP` | `+0x2324` | new |
| `Tq_08_RPM_NGP` | `+0x2344` | new |
| `Tq_09_RPM_NGP` | `+0x2364` | new |
| `Tq_10_RPM_NGP` | `+0x2384` | new |
| `Tq_11_RPM_NGP` | `+0x23a4` | new |
| `Tq_12_RPM_NGP` | `+0x23c4` | new |
| `Tq_13_RPM_NGP` | `+0x23e4` | new |
| `Tq_14_RPM_NGP` | `+0x2404` | new |
| `Tq_15_RPM_NGP` | `+0x2424` | new |
| `Tq_00_NGP` | `+0x2254` | shares `VE00` |
| `Tq_01_NGP` | `+0x2274` | shares `VE01` |
| `Tq_02_NGP` | `+0x2294` | shares `VE02` |
| `Tq_03_NGP` | `+0x22b4` | shares `VE03` |
| `Tq_04_NGP` | `+0x22d4` | shares `VE04` |
| `Tq_05_NGP` | `+0x22f4` | shares `VE05` |
| `Tq_06_NGP` | `+0x2314` | shares `VE06` |
| `Tq_07_NGP` | `+0x2334` | shares `VE07` |
| `Tq_08_NGP` | `+0x2354` | shares `VE08` |
| `Tq_09_NGP` | `+0x2374` | shares `VE09` |
| `Tq_10_NGP` | `+0x2394` | shares `VE10` |
| `Tq_11_NGP` | `+0x23b4` | shares `VE11` |
| `Tq_12_NGP` | `+0x23d4` | shares `VE12` |
| `Tq_13_NGP` | `+0x23f4` | shares `VE13` |
| `Tq_14_NGP` | `+0x2414` | shares `VE14` |
| `Tq_15_NGP` | `+0x2434` | shares `VE15` |
| `TurboTurbineBackPressure_00_AV_NGP` | `+0x2444` | new |
| `TurboTurbineBackPressure_01_AV_NGP` | `+0x2464` | new |
| `TurboTurbineBackPressure_02_AV_NGP` | `+0x2484` | new |
| `TurboTurbineBackPressure_03_AV_NGP` | `+0x24a4` | new |
| `TurboTurbineBackPressure_04_AV_NGP` | `+0x24c4` | new |
| `TurboTurbineBackPressure_05_AV_NGP` | `+0x24e4` | new |
| `TurboTurbineBackPressure_06_AV_NGP` | `+0x2504` | new |
| `TurboTurbineBackPressure_07_AV_NGP` | `+0x2524` | new |
| `TurboTurbineBackPressure_08_AV_NGP` | `+0x2544` | new |
| `TurboTurbineBackPressure_09_AV_NGP` | `+0x2564` | new |
| `TurboTurbineBackPressure_10_AV_NGP` | `+0x2584` | new |
| `TurboTurbineBackPressure_11_AV_NGP` | `+0x25a4` | new |
| `TurboTurbineBackPressure_12_AV_NGP` | `+0x25c4` | new |
| `TurboTurbineBackPressure_13_AV_NGP` | `+0x25e4` | new |
| `TurboTurbineBackPressure_14_AV_NGP` | `+0x2604` | new |
| `TurboTurbineBackPressure_15_AV_NGP` | `+0x2624` | new |
| `TurboTurbineBackPressure_00_NGP` | `+0x2454` | new |
| `TurboTurbineBackPressure_01_NGP` | `+0x2474` | new |
| `TurboTurbineBackPressure_02_NGP` | `+0x2494` | new |
| `TurboTurbineBackPressure_03_NGP` | `+0x24b4` | new |
| `TurboTurbineBackPressure_04_NGP` | `+0x24d4` | new |
| `TurboTurbineBackPressure_05_NGP` | `+0x24f4` | new |
| `TurboTurbineBackPressure_06_NGP` | `+0x2514` | new |
| `TurboTurbineBackPressure_07_NGP` | `+0x2534` | new |
| `TurboTurbineBackPressure_08_NGP` | `+0x2554` | new |
| `TurboTurbineBackPressure_09_NGP` | `+0x2574` | new |
| `TurboTurbineBackPressure_10_NGP` | `+0x2594` | new |
| `TurboTurbineBackPressure_11_NGP` | `+0x25b4` | new |
| `TurboTurbineBackPressure_12_NGP` | `+0x25d4` | new |
| `TurboTurbineBackPressure_13_NGP` | `+0x25f4` | new |
| `TurboTurbineBackPressure_14_NGP` | `+0x2614` | new |
| `TurboTurbineBackPressure_15_NGP` | `+0x2634` | new |
| `CylinderVolume_NGP` | `+0x2644` | new |
| `DissipatedHeatFraction_NGP` | `+0x269c` | new |
| `InletPipeAirFlowCoefficient_NGP` | `+0x26ac` | new |
| `OutletPipeAirFlowCoefficient_NGP` | `+0x26bc` | new |
| `InletPipeArea_NGP` | `+0x26cc` | new |
| `OutletPipeArea_NGP` | `+0x26dc` | new |
| `IdleSpeedThrottle_NGP` | `+0x26fc` | new |
| `TurboHosingDamageLossCoefficient_NGP` | `+0x270c` | new |
| `EngineHeatTransferCoefficient_NGP` | `+0x2728` | new |
| `EngineCoolantDuctArea_NGP` | `+0x2738` | new |
| `EngineCoolantHeatStateMassUpperBound_NGP` | `+0x2758` | new |
| `ThermostatOpeningTemperature_NGP` | `+0x2768` | new |
| `WaterPumpMassFlowRate_NGP` | `+0x2778` | new |
| `RadiatorCoolantHeatStateMassUpperBound_NGP` | `+0x2790` | new |
| `RadiatorCoolantMassLowerBound_NGP` | `+0x27a0` | new |
| `RadiatorCoolantMassUpperBound_NGP` | `+0x27b0` | new |
| `RadiatorArea_NGP` | `+0x27c0` | new |
| `BaseAirCooling_NGP` | `+0x27d0` | new |
| `VelocityAirCooling_NGP` | `+0x27e0` | new |
| `OilCapacity_NGP` | `+0x2810` | new |
| `OilPumpOutput_NGP` | `+0x2820` | new |
| `TorqueCut_NGP` | `+0x2830` | new |
| `EngineMass_NGP` | `+0x2870` | new |
| `CylinderOilFrictionTorqueCoefficient_NGP` | `+0x2840` | new |
| `HighTemperatureFrictionLossTorqueCoefficient_NGP` | `+0x2850` | new |
| `FrictionLossTemperatureThreshold_NGP` | `+0x2860` | new |

### CMaterialManager (dict `0x1659ad8`, NGP table `0x10172350`, 149 records: 134 stock, 15 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `Bendable_01_NGP` | `+0x1146c` | new |
| `Bendable_02_NGP` | `+0x115cc` | new |
| `Bendable_03_NGP` | `+0x1172c` | new |
| `Bendable_04_NGP` | `+0x1188c` | new |
| `Bendable_05_NGP` | `+0x119ec` | new |
| `Bendable_06_NGP` | `+0x11b4c` | new |
| `Bendable_07_NGP` | `+0x11cac` | new |
| `Bendable_08_NGP` | `+0x11e0c` | new |
| `Bendable_09_NGP` | `+0x11f6c` | new |
| `Bendable_10_NGP` | `+0x120cc` | new |
| `Bendable_11_NGP` | `+0x1222c` | new |
| `Bendable_12_NGP` | `+0x1238c` | new |
| `Bendable_13_NGP` | `+0x124ec` | new |
| `Bendable_14_NGP` | `+0x1264c` | new |
| `Bendable_15_NGP` | `+0x127ac` | new |

### CTyre::CDef (dict `0x165f1b0`, NGP table `0x10172cb0`, 16 records: 1 stock, 15 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `CarcassStiffnessLateral_NGP` | `+0x40` | `CarcassStiffnessLateral` (renamed) |
| `CarcassStiffnessTorsional_NGP` | `+0x50` | `CarcassStiffnessTorsional` (renamed) |
| `CarcassWidth_NGP` | `+0xc0` | `CarcassWidth` (renamed) |
| `CarcassRadius_NGP` | `+0x70` | `CarcassRadius` (renamed) |
| `TreadDepth_NGP` | `+0x80` | `TreadDepth` (renamed) |
| `TreadWidth_NGP` | `+0xa0` | `TreadWidth` (renamed) |
| `TreadCoverage_NGP` | `+0xb0` | `TreadCoverage` (renamed) |
| `Performance_NGP` | `+0x30` | `ReferenceFriction` (renamed) |
| `HubOffset_NGP` | `+0x60` | `HubOffset` (renamed) |
| `CarcassThickness_NGP` | `+0x90` | new |
| `SideWallHeight_NGP` | `+0xe0` | new |
| `PressureRatio_NGP` | `+0x20` | new |
| `CarcassStiffnessVertical_NGP` | `+0xd0` | new |
| `PressureFuncFtr_NGP` | `+0xf0` | new |
| `FlexHeatFtr_NGP` | `+0x100` | new |

### CVehicleControlUnit::CDef (dict `0x165f214`, NGP table `0x10172db8`, 26 records: 23 stock, 3 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `UpShiftDelay_NGP` | `+0x14c` | new |
| `DownShiftDelay_NGP` | `+0x15c` | new |
| `ClutchHelpDecrement_NGP` | `+0x16c` | new |

### CWheel::CDef (dict `0x165f2cc`, NGP table `0x10172f68`, 33 records: 26 stock, 7 NGP)

| key | member | stock key it replaces |
| --- | --- | --- |
| `vecPosOffset_NGP` | `+0x2f0` | `vecDriveAxleJointPos_VF` (renamed) |
| `BumpStopRebound_NGP` | `+0x1f0` | new |
| `TopMountDamageForceThreshold_NGP` | `+0x924` | new |
| `TopMountDamageMaxForce_NGP` | `+0x934` | new |
| `AxisInclinationDamageAngle_NGP` | `+0x944` | new |
| `HubDamageTorqueThreshold_NGP` | `+0x954` | new |
| `HubDamageMaxTorque_NGP` | `+0x964` | new |

## Renamed in place in the stock tables

Read back from the stock tables in `snap1.snap-10880`.

| class | key | stock key | member |
| --- | --- | --- | --- |
| CDrive::CSetupOptions | `FrontBrakeVelSlope_NGP` | `FrontBrakes` | `+0xdbc` |
| CDrive::CSetupOptions | `RearBrakeVelSlope_NGP` | `RearBrakes` | `+0xf3c` |
| CDrive::CSetupOptions | `BumpStopStiffnessMin_NGP` | `FrontBrake1Mass` | `+0xe7c` |
| CDrive::CSetupOptions | `BumpStopStiffnessMax_NGP` | `FrontBrake1Inertia` | `+0xe8c` |
| CDrive::CSetupOptions | `BumpStopStiffnessStep_NGP` | `FrontBrake1Diameter` | `+0xe9c` |
| CDrive::CSetupOptions | `BumpStopDampingMin_NGP` | `FrontBrake1RadiusEff` | `+0xeac` |
| CDrive::CSetupOptions | `BumpStopDampingMax_NGP` | `FrontBrake1SpecHeat` | `+0xebc` |
| CDrive::CSetupOptions | `BumpStopDampingStep_NGP` | `FrontBrake1HeatAbsRatio` | `+0xecc` |
| CDrive::CSetupOptions | `CenterDiffMapLockPctMin_NGP` | `Gear0OptionsMin` | `+0xbac` |
| CDrive::CSetupOptions | `CenterDiffMapLockPctMax_NGP` | `Gear0OptionsMax` | `+0xbbc` |
| CDrive::CSetupOptions | `FrontDiffMapLockPctMin_NGP` | `Gear1OptionsMin` | `+0xbcc` |
| CDrive::CSetupOptions | `FrontDiffMapLockPctMax_NGP` | `Gear1OptionsMax` | `+0xbdc` |
| CDrive::CSetupOptions | `RearDiffMapLockPctMin_NGP` | `Gear2OptionsMin` | `+0xbec` |
| CDrive::CSetupOptions | `RearDiffMapLockPctMax_NGP` | `Gear2OptionsMax` | `+0xbfc` |
| CDrive::CSetupOptions | `Features_NGP` | `Gear3OptionsMin` | `+0xc0c` |
| CDrive::CSetupOptions | `TorqueSplit_NGP` | `Gear3OptionsMax` | `+0xc1c` |
| CDrive::CSetupOptions | `FrontTyreCoolingFtr_NGP` | `Gear4OptionsMin` | `+0xc2c` |
| CDrive::CSetupOptions | `RearTyreCoolingFtr_NGP` | `Gear4OptionsMax` | `+0xc3c` |
| CDrive::CSetupOptions | `TyreHeatFlowFtr_NGP` | `Gear5OptionsMin` | `+0xc4c` |
| CDrive::CSetupOptions | `DriveTrainLoss_NGP` | `Gear5OptionsMax` | `+0xc5c` |
| CDrive::CSetupOptions | `TyrePerformancePctT_NGP` | `Gear6OptionsMin` | `+0xc6c` |
| CDrive::CSetupOptions | `TyrePerformancePctG_NGP` | `Gear6OptionsMax` | `+0xc7c` |
| CDrive::CSetupOptions | `TyrePerformancePctS_NGP` | `Gear7OptionsMin` | `+0xc8c` |
| CEngine::CSetup | `Features_NGP` | `Dummy` | `+0xc` |
| CEngine::CSetupOptions | `IdleRPM_NGP` | `DummyMin` | `+0xc` |
| CEngine::CSetupOptions | `MaxTorque_NGP` | `DummyMax` | `+0x1c` |
| CEngine::CSetupOptions | `BrakeCoefficient_NGP` | `DummyStep` | `+0x2c` |
| CPhysics::CDef | `PhysicsRevision_NGP` | `MaxExpirableCount` | `+0x16070` |
