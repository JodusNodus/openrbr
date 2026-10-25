"""Records the entities_b4 set (harness/vector_specs/entities_b4.py) for harness/vcu_vectors.py. The load-time dumps
(snapshot version 2) are only accepted under --set entities, so this runs the same main() with the B4 specs registered
under that name. Usage as vcu_vectors.py: RBR_REMOTE=<dir> python3 -I harness/record_entities_b4.py <snapshot> <out> --set entities.

--no-launch (this script only): with --fuzz, keep the launch control off and the step's own fuzzers (GearboxUpdate,
LaunchClutch, FlagDispatch, StepBasic, QuerySlip) off. vcu_vectors.py turns the launch on whenever a fuzz seed is given,
and those fuzzers change the step's state; on the step-40 snapshot that sends the step off its recorded path (an
unstubbed call). The spec's own fuzz (the "fuzz" key) still runs, so the entities set's records see the fuzzed inputs."""
import os
import sys

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import vcu_vectors  # noqa: E402

vcu_vectors.SETS["entities"] = vcu_vectors.SETS["entities_b4"]

if "--no-launch" in sys.argv:
    sys.argv.remove("--no-launch")
    _Recorder = vcu_vectors.Recorder

    class NoLaunchRecorder(_Recorder):
        def __init__(self, machine, limit, launch=False, fuzz=None):
            super().__init__(machine, limit, launch, fuzz)
            self.launch = launch

        def fuzz_gearbox(self, mu, this):
            pass

        def fuzz_clutch(self, mu, this):
            pass

        def fuzz_dispatch(self, mu, this, packet):
            pass

        def fuzz_step(self, mu, this):
            pass

        def fuzz_slip(self, mu, this):
            pass

    vcu_vectors.Recorder = NoLaunchRecorder

vcu_vectors.main()
