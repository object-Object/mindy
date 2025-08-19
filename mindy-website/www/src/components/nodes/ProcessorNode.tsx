import { ActionIcon, Card, Group, Textarea } from "@mantine/core";
import {
    useNodeConnections,
    useReactFlow,
    type Node,
    type NodeProps,
} from "@xyflow/react";
import { useEffect, useState } from "react";
import { FaXmark, FaCheck } from "react-icons/fa6";

import { ProcessorKind } from "mindy-website";

import { useLogicVM } from "../../hooks";
import type { LogicVMNode } from "../LogicVMFlow";
import BuildingNode, { type BuildingNodeData } from "./BuildingNode";
import classes from "./ProcessorNode.module.css";

type ProcessorNodeData = BuildingNodeData & {
    kind: ProcessorKind;
    defaultCode?: string;
    error?: string;
};

export type ProcessorNodeType = Node<ProcessorNodeData, "processor">;

export default function ProcessorNode(props: NodeProps<ProcessorNodeType>) {
    const {
        data: { position, kind, defaultCode = "", error },
    } = props;

    const vm = useLogicVM();

    const [code, setCode] = useState(defaultCode);
    const [editCode, setEditCode] = useState(defaultCode);

    const connections = useNodeConnections({ handleType: "source" });
    const reactFlow = useReactFlow<LogicVMNode>();

    useEffect(() => {
        vm.postMessage({
            type: "addProcessor",
            position,
            kind,
        });

        return () => {
            vm.postMessage({ type: "removeBuilding", position });
        };
    }, [vm, position, kind]);

    useEffect(() => {
        const links = connections.flatMap((value) => {
            const position = reactFlow.getNode(value.target)?.data.position;
            return position != null ? [position] : [];
        });
        vm.postMessage({
            type: "setProcessorCode",
            position,
            code,
            links: new Uint32Array(links),
        });
    }, [vm, position, code, connections, reactFlow]);

    return (
        <BuildingNode linkSource {...props}>
            <Card.Section p="xs">
                <Textarea
                    className={`${classes.input} nodrag nopan nowheel`}
                    value={editCode}
                    resize="both"
                    autosize
                    maxRows={32}
                    size="xs"
                    onChange={(e) => setEditCode(e.currentTarget.value)}
                    error={error}
                    errorProps={{
                        maw: "25vw",
                        pb: 4,
                    }}
                />
                <Group justify="flex-end" pt={2} gap={4}>
                    <ActionIcon
                        className={`${classes.button} nodrag nopan`}
                        variant="filled"
                        color="red"
                        size="sm"
                        disabled={code === editCode}
                        onClick={() => setEditCode(code)}
                    >
                        <FaXmark />
                    </ActionIcon>
                    <ActionIcon
                        className={`${classes.button} nodrag nopan`}
                        variant="filled"
                        size="sm"
                        disabled={code === editCode}
                        onClick={() => setCode(editCode)}
                    >
                        <FaCheck />
                    </ActionIcon>
                </Group>
            </Card.Section>
        </BuildingNode>
    );
}
