import init, { init_logging, WebLogicVM } from "mindy-website";

import type {
    BuildingUpdateMap,
    VMWorkerRequest,
    VMWorkerResponse,
} from "./vm";

declare function postMessage(
    message: VMWorkerResponse,
    transfer?: Transferable,
): void;

// set up VM

void init().then(() => {
    init_logging();
    const vm = new WebLogicVM(
        60,
        (
            position: number,
            buildingType: keyof BuildingUpdateMap,
            update: unknown,
        ) => {
            postMessage({
                type: "buildingUpdated",
                position,
                buildingType,
                // hack
                update: update as BuildingUpdateMap[typeof buildingType],
            });
        },
    );

    onmessage = ({ data: request }: MessageEvent<VMWorkerRequest>) => {
        console.debug("Got request:", request);

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

            case "addMemory": {
                const { position, kind } = request;

                vm.add_memory(position, kind);
                postMessage({
                    type: "buildingAdded",
                    position,
                    name: vm.building_name(position),
                });

                break;
            }

            case "addMessage": {
                const { position } = request;

                vm.add_message(position);
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

            case "addSwitch": {
                const { position } = request;

                vm.add_switch(position);
                postMessage({
                    type: "buildingAdded",
                    position,
                    name: vm.building_name(position),
                });

                break;
            }

            case "setMessageText": {
                const { position, value } = request;
                vm.set_message_text(position, value);
                break;
            }

            case "setProcessorCode": {
                const { position, code, links } = request;

                let error = undefined;
                let linkNames = undefined;
                try {
                    linkNames = vm.set_processor_config(position, code, links);
                } catch (e: unknown) {
                    error = String(e);
                }
                postMessage({
                    type: "buildingUpdated",
                    position,
                    buildingType: "processor",
                    update: {
                        links: linkNames,
                        error,
                    },
                });

                break;
            }

            case "setSwitchEnabled": {
                const { position, value } = request;
                vm.set_switch_enabled(position, value);
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
    onmessageerror = (event) => {
        console.error("Failed to deserialize message from main thread:", event);
    };
    onerror = (event) => {
        console.error("Got error from main thread:", event);
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
