import { Center, Switch } from "@mantine/core";
import type { NodeProps, Node } from "@xyflow/react";
import { useEffect, useState } from "react";

import { useLogicVM } from "../../hooks";
import type { BuildingNodeData } from "./BuildingNode";
import BuildingNode from "./BuildingNode";

export type SwitchNodeType = Node<BuildingNodeData, "switch">;

export default function SwitchNode(props: NodeProps<SwitchNodeType>) {
    const {
        data: { position },
    } = props;

    const vm = useLogicVM();

    const [checked, setChecked] = useState(false);

    useEffect(() => {
        vm.postMessage({ type: "addSwitch", position });

        return () => {
            vm.postMessage({ type: "removeBuilding", position });
        };
    }, [vm, position]);

    return (
        <BuildingNode buildingType="switch" onUpdate={setChecked} {...props}>
            <Center>
                <Switch
                    checked={checked}
                    onChange={(e) =>
                        vm.postMessage({
                            type: "setSwitchEnabled",
                            position,
                            value: e.currentTarget.checked,
                        })
                    }
                />
            </Center>
        </BuildingNode>
    );
}
