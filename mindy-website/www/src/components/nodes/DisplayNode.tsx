import { Box, Card } from "@mantine/core";
import type { Node, NodeProps } from "@xyflow/react";
import { useEffect, useRef, useState } from "react";

import { WebLogicVM } from "mindy-website";

import BuildingNode from "./BuildingNode";
import classes from "./DisplayNode.module.css";

type DisplayNodeData = {
    vm: WebLogicVM;
    position: number;
    displayWidth: number;
    displayHeight: number;
};
export type DisplayNodeType = Node<DisplayNodeData, "display">;

export default function DisplayNode(props: NodeProps<DisplayNodeType>) {
    const {
        data: { vm, position, displayWidth, displayHeight },
    } = props;

    const displayRef = useRef<HTMLDivElement>(null);
    const positionRef = useRef<number>(null);
    const [name, setName] = useState<string>();

    useEffect(() => {
        if (position !== positionRef.current) {
            // if this isn't here, it adds multiple canvases in dev mode for some reason
            displayRef.current!.replaceChildren();

            vm.add_display(
                position,
                displayWidth,
                displayHeight,
                displayRef.current!,
            );

            setName(vm.building_name(position));

            positionRef.current = position;
        }
    }, [vm, position, displayWidth, displayHeight]);

    return (
        <BuildingNode name={name} {...props}>
            <Card.Section>
                <Box className={classes.display} ref={displayRef} p="xs"></Box>
            </Card.Section>
        </BuildingNode>
    );
}
