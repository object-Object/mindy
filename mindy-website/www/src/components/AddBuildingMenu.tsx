import { ActionIcon, Menu } from "@mantine/core";
import { useReactFlow, useStoreApi } from "@xyflow/react";
import { FaPlus } from "react-icons/fa6";

import { DisplayKind, ProcessorKind } from "mindy-website";

import { createNode } from "../utils";
import type { LogicVMNode } from "./LogicVMFlow";

export default function AddBuildingMenu() {
    const reactFlow = useReactFlow<LogicVMNode>();
    const store = useStoreApi<LogicVMNode>();

    // https://github.com/xyflow/xyflow/issues/3391#issuecomment-2590673868
    function locateNode() {
        const { domNode } = store.getState();
        const boundingRect = domNode?.getBoundingClientRect();
        if (boundingRect == null) {
            return [null, null];
        }

        const center = reactFlow.screenToFlowPosition({
            x: boundingRect.x + boundingRect.width / 2,
            y: boundingRect.y + boundingRect.height / 2,
        });

        return [center, { x: reactFlow.getNodes().length, y: 0 }];
    }

    function addProcessor(kind: ProcessorKind) {
        const [nodePosition, buildingPosition] = locateNode();
        if (nodePosition == null || buildingPosition == null) return;

        reactFlow.addNodes(
            createNode({
                type: "processor",
                position: nodePosition,
                data: {
                    position: buildingPosition,
                    kind,
                },
            }),
        );
    }

    function addDisplay(
        kind: DisplayKind,
        displayWidth: number,
        displayHeight?: number,
    ) {
        const [nodePosition, buildingPosition] = locateNode();
        if (nodePosition == null || buildingPosition == null) return;

        reactFlow.addNodes(
            createNode({
                type: "display",
                position: nodePosition,
                data: {
                    position: buildingPosition,
                    kind,
                    displayWidth,
                    displayHeight: displayHeight ?? displayWidth,
                },
            }),
        );
    }

    return (
        <Menu position="top-end">
            <Menu.Target>
                <ActionIcon
                    className="nodrag nopan"
                    size="xl"
                    radius="xl"
                    pos="absolute"
                    right={0}
                    bottom={0}
                    m="md"
                    style={{ zIndex: 999 }}
                >
                    <FaPlus />
                </ActionIcon>
            </Menu.Target>

            <Menu.Dropdown className="nodrag nopan nowheel">
                <Menu.Label>Processors</Menu.Label>

                <Menu.Item onClick={() => addProcessor(ProcessorKind.Micro)}>
                    Micro Processor
                </Menu.Item>
                <Menu.Item onClick={() => addProcessor(ProcessorKind.Logic)}>
                    Logic Processor
                </Menu.Item>
                <Menu.Item onClick={() => addProcessor(ProcessorKind.Hyper)}>
                    Hyper Processor
                </Menu.Item>
                <Menu.Item onClick={() => addProcessor(ProcessorKind.World)}>
                    World Processor
                </Menu.Item>

                <Menu.Label>Displays</Menu.Label>

                <Menu.Item onClick={() => addDisplay(DisplayKind.Logic, 80)}>
                    Logic Display
                </Menu.Item>
                <Menu.Item onClick={() => addDisplay(DisplayKind.Large, 176)}>
                    Large Logic Display
                </Menu.Item>
                <Menu.Item onClick={() => addDisplay(DisplayKind.Tiled, 256)}>
                    Tiled Logic Display (8x8)
                </Menu.Item>
                <Menu.Item onClick={() => addDisplay(DisplayKind.Tiled, 512)}>
                    Tiled Logic Display (16x16)
                </Menu.Item>
            </Menu.Dropdown>
        </Menu>
    );
}
