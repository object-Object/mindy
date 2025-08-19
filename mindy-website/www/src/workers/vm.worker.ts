import init, { init_logging, WebLogicVM } from "mindy-website";

import type { VMWorkerRequest, VMWorkerResponse } from "./vm";

declare function postMessage(
    message: VMWorkerResponse,
    transfer?: Transferable,
): void;

// set up VM

void init().then(() => {
    init_logging();
    const vm = new WebLogicVM(60);

    onmessage = ({ data: request }: MessageEvent<VMWorkerRequest>) => {
        switch (request.type) {
            case "addDisplay": {
                const { position, kind, width, height, canvas } = request;

                vm.add_display(position, kind, width, height, canvas);
                postMessage({
                    type: "buildingAdded",
                    position,
                    name: vm.building_name(position),
                });

                break;
            }

            case "addProcessor": {
                const { position, kind } = request;

                vm.add_processor(position, kind);
                postMessage({
                    type: "buildingAdded",
                    position,
                    name: vm.building_name(position),
                });

                break;
            }

            case "setProcessorCode": {
                const { position, code, links } = request;

                let error = undefined;
                try {
                    vm.set_processor_config(position, code, links);
                } catch (e: unknown) {
                    error = String(e);
                }
                postMessage({
                    type: "processorCodeSet",
                    position,
                    links: vm.processor_links(position),
                    error,
                });

                break;
            }

            case "removeBuilding": {
                vm.remove_building(request.position);
                break;
            }

            case "setTargetFPS": {
                vm.set_target_fps(request.target);
                break;
            }
        }
    };

    // tell the main thread that we're ready to receive requests

    postMessage({ type: "ready" });

    // start main VM loop

    const doTick = () => {
        vm.do_tick();
        requestAnimationFrame(doTick);
    };
    requestAnimationFrame(doTick);
});
