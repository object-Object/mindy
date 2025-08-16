import { ActionIcon, Card, Group, Textarea } from "@mantine/core";
import {
    useNodeConnections,
    useReactFlow,
    type Node,
    type NodeProps,
} from "@xyflow/react";
import { useEffect, useRef, useState } from "react";
import { FaXmark, FaCheck } from "react-icons/fa6";

import { ProcessorKind, WebLogicVM } from "mindy-website";

import type { CustomNodeType } from "../LogicVMFlow";
import BuildingNode from "./BuildingNode";
import classes from "./ProcessorNode.module.css";

type ProcessorNodeData = {
    vm: WebLogicVM;
    position: number;
    kind: ProcessorKind;
    defaultCode?: string;
};
export type ProcessorNodeType = Node<ProcessorNodeData, "processor">;

export default function ProcessorNode(props: NodeProps<ProcessorNodeType>) {
    const {
        data: { vm, position, kind, defaultCode = "" },
    } = props;

    const positionRef = useRef<number>(null);

    const [name, setName] = useState<string>();
    const [code, setCode] = useState(defaultCode);
    const [editCode, setEditCode] = useState(defaultCode);
    const [error, setError] = useState<string>();

    const connections = useNodeConnections({ handleType: "source" });
    const reactFlow = useReactFlow<CustomNodeType>();

    useEffect(() => {
        if (position !== positionRef.current) {
            vm.add_processor(position, kind, defaultCode);

            setName(vm.building_name(position));

            positionRef.current = position;
        }
    }, [vm, position, kind, defaultCode]);

    useEffect(() => {
        try {
            vm.set_processor_config(
                position,
                code,
                new Uint32Array(
                    connections
                        .map(
                            (value) =>
                                reactFlow.getNode(value.target)?.data.position,
                        )
                        .filter((value) => value != null),
                ),
            );

            setError(undefined);
        } catch (e: unknown) {
            setError(String(e));
        }
    }, [vm, position, code, connections, reactFlow]);

    return (
        <BuildingNode name={name} linkSource {...props}>
            <Card.Section p="xs">
                <Textarea
                    className={`${classes.input} nodrag nopan nowheel`}
                    value={editCode}
                    resize="both"
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
