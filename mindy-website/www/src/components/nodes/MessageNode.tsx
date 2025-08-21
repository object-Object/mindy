import { ActionIcon, Group, Textarea } from "@mantine/core";
import type { NodeProps, Node } from "@xyflow/react";
import { useEffect, useState } from "react";
import { FaCheck } from "react-icons/fa6";

import { useLogicVM } from "../../hooks";
import type { BuildingNodeData } from "./BuildingNode";
import BuildingNode from "./BuildingNode";
import classes from "./MessageNode.module.css";

export type MessageNodeType = Node<BuildingNodeData, "message">;

export default function MessageNode(props: NodeProps<MessageNodeType>) {
    const {
        data: { position },
    } = props;

    const vm = useLogicVM();

    const [value, setValue] = useState("");
    const [editing, setEditing] = useState(false);

    useEffect(() => {
        vm.postMessage({ type: "addMessage", position });

        return () => {
            vm.postMessage({ type: "removeBuilding", position });
        };
    }, [vm, position]);

    return (
        <BuildingNode
            buildingType="message"
            onUpdate={(newValue) => {
                setValue(newValue);
                setEditing(false);
            }}
            {...props}
        >
            <Textarea
                className="nodrag nopan nowheel"
                value={value}
                onChange={(e) => {
                    setValue(e.currentTarget.value);
                    setEditing(true);
                }}
                w={256}
                autosize
                minRows={4}
                maxRows={16}
                maxLength={256}
                size="xs"
            />
            <Group justify="flex-end" pt={2} gap={4}>
                <ActionIcon
                    className={`${classes.button} nodrag nopan`}
                    variant="filled"
                    size="sm"
                    ml="auto"
                    disabled={!editing}
                    onClick={() =>
                        vm.postMessage({
                            type: "setMessageText",
                            position,
                            value,
                        })
                    }
                >
                    <FaCheck />
                </ActionIcon>
            </Group>
        </BuildingNode>
    );
}
