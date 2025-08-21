import { ActionIcon, Menu } from "@mantine/core";
import { useReactFlow, useStoreApi, type XYPosition } from "@xyflow/react";
import { FaPlus } from "react-icons/fa6";

import {
    display_size,
    DisplayKind,
    memory_size,
    MemoryKind,
    processor_size,
    ProcessorKind,
} from "mindy-website";

import { createNode } from "../utils";
import type { LogicVMNode } from "./LogicVMFlow";

export default function AddBuildingMenu() {
    const reactFlow = useReactFlow<LogicVMNode>();
    const store = useStoreApi<LogicVMNode>();

    function createBuildingNode<N, D>(
        node: N & { size: number; data: D },
    ): (N & {
        id: string;
        position: XYPosition;
        data: D & { position: number };
    })[] {
        const { domNode } = store.getState();
        const boundingRect = domNode?.getBoundingClientRect();
        if (boundingRect == null) {
            return [];
        }

        const nodePosition = reactFlow.screenToFlowPosition({
            x: boundingRect.x + boundingRect.width / 2,
            y: boundingRect.y + boundingRect.height / 2,
        });

        // calculate a position such that all nodes of a given size are at the same y position without overlapping
        const buildingPosition = {
            // leave enough room to the left assuming all nodes are in the same row as this one
            x: reactFlow.getNodes().length * node.size,
            // the sum of the first n natural numbers is n*(n+1)/2
            // for a given size, we need to leave room below for all sizes < size
            // so y should be the sum of all sizes < size
            // ie. y = (size - 1) * (size - 1 + 1) / 2
            //       = (size - 1) * size / 2
            y: ((node.size - 1) * node.size) / 2,
        };

        return [
            createNode({
                ...node,
                position: nodePosition,
                data: {
                    ...node.data,
                    position: buildingPosition,
                },
            }),
        ];
    }

    function addDisplay(
        kind: DisplayKind,
        displayWidth: number,
        displayHeight?: number,
    ) {
        reactFlow.addNodes(
            createBuildingNode({
                type: "display",
                size: display_size(kind),
                data: {
                    kind,
                    displayWidth,
                    displayHeight: displayHeight ?? displayWidth,
                },
            }),
        );
    }

    function addMemory(kind: MemoryKind) {
        reactFlow.addNodes(
            createBuildingNode({
                type: "memory",
                size: memory_size(kind),
                data: { kind },
            }),
        );
    }

    function addProcessor(kind: ProcessorKind) {
        reactFlow.addNodes(
            createBuildingNode({
                type: "processor",
                size: processor_size(kind),
                data: {
                    kind,
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
                <Menu.Sub position="left" offset={-1}>
                    <Menu.Sub.Target>
                        <Menu.Sub.Item>Displays</Menu.Sub.Item>
                    </Menu.Sub.Target>

                    <Menu.Sub.Dropdown>
                        <Menu.Item
                            onClick={() => addDisplay(DisplayKind.Logic, 80)}
                        >
                            Logic Display
                        </Menu.Item>

                        <Menu.Item
                            onClick={() => addDisplay(DisplayKind.Large, 176)}
                        >
                            Large Logic Display
                        </Menu.Item>

                        <Menu.Item
                            onClick={() => addDisplay(DisplayKind.Tiled, 256)}
                        >
                            Tiled Logic Display (8x8)
                        </Menu.Item>

                        <Menu.Item
                            onClick={() => addDisplay(DisplayKind.Tiled, 512)}
                        >
                            Tiled Logic Display (16x16)
                        </Menu.Item>
                    </Menu.Sub.Dropdown>
                </Menu.Sub>

                <Menu.Sub position="left" offset={-1}>
                    <Menu.Sub.Target>
                        <Menu.Sub.Item>Memory</Menu.Sub.Item>
                    </Menu.Sub.Target>

                    <Menu.Sub.Dropdown>
                        <Menu.Item onClick={() => addMemory(MemoryKind.Cell)}>
                            Memory Cell
                        </Menu.Item>

                        <Menu.Item onClick={() => addMemory(MemoryKind.Bank)}>
                            Memory Bank
                        </Menu.Item>

                        <Menu.Item
                            onClick={() => addMemory(MemoryKind.WorldCell)}
                        >
                            World Cell
                        </Menu.Item>
                    </Menu.Sub.Dropdown>
                </Menu.Sub>

                <Menu.Sub position="left" offset={-1}>
                    <Menu.Sub.Target>
                        <Menu.Sub.Item>Processors</Menu.Sub.Item>
                    </Menu.Sub.Target>

                    <Menu.Sub.Dropdown>
                        <Menu.Item
                            onClick={() => addProcessor(ProcessorKind.Micro)}
                        >
                            Micro Processor
                        </Menu.Item>

                        <Menu.Item
                            onClick={() => addProcessor(ProcessorKind.Logic)}
                        >
                            Logic Processor
                        </Menu.Item>

                        <Menu.Item
                            onClick={() => addProcessor(ProcessorKind.Hyper)}
                        >
                            Hyper Processor
                        </Menu.Item>

                        <Menu.Item
                            onClick={() => addProcessor(ProcessorKind.World)}
                        >
                            World Processor
                        </Menu.Item>
                    </Menu.Sub.Dropdown>
                </Menu.Sub>

                <Menu.Sub position="left" offset={-1}>
                    <Menu.Sub.Target>
                        <Menu.Sub.Item>Other</Menu.Sub.Item>
                    </Menu.Sub.Target>

                    <Menu.Sub.Dropdown>
                        <Menu.Item
                            onClick={() =>
                                reactFlow.addNodes(
                                    createBuildingNode({
                                        type: "message",
                                        size: 1,
                                        data: {},
                                    }),
                                )
                            }
                        >
                            Message
                        </Menu.Item>

                        <Menu.Item
                            onClick={() =>
                                reactFlow.addNodes(
                                    createBuildingNode({
                                        type: "switch",
                                        size: 1,
                                        data: {},
                                    }),
                                )
                            }
                        >
                            Switch
                        </Menu.Item>
                    </Menu.Sub.Dropdown>
                </Menu.Sub>
            </Menu.Dropdown>
        </Menu>
    );
}
