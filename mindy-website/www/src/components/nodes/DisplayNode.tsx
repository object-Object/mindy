import { Box, Card } from "@mantine/core";
import type { Node, NodeProps } from "@xyflow/react";
import { useEffect, useRef } from "react";

import type { DisplayKind } from "mindy-website";

import { useLogicVM } from "../../hooks";
import BuildingNode, { type BuildingNodeData } from "./BuildingNode";
import classes from "./DisplayNode.module.css";

type DisplayNodeData = BuildingNodeData & {
    kind: DisplayKind;
    displayWidth: number;
    displayHeight: number;
};

export type DisplayNodeType = Node<DisplayNodeData, "display">;

export default function DisplayNode(props: NodeProps<DisplayNodeType>) {
    const {
        data: { position, kind, displayWidth, displayHeight },
    } = props;

    const vm = useLogicVM();

    const displayRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        const canvas = document.createElement("canvas");
        displayRef.current!.replaceChildren(canvas);

        const offscreenCanvas = canvas.transferControlToOffscreen();
        vm.postMessage(
            {
                type: "addDisplay",
                position,
                kind,
                width: displayWidth,
                height: displayHeight,
                canvas: offscreenCanvas,
            },
            [offscreenCanvas],
        );

        return () => {
            vm.postMessage({ type: "removeBuilding", position });
        };
    }, [vm, position, kind, displayWidth, displayHeight]);

    return (
        <BuildingNode {...props}>
            <Card.Section p="xs">
                <Box
                    className={classes.display}
                    ref={displayRef}
                    style={{
                        width: displayWidth,
                        height: displayHeight,
                    }}
                ></Box>
            </Card.Section>
        </BuildingNode>
    );
}
