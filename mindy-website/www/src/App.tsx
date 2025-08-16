import { Center, Loader, MantineProvider } from "@mantine/core";
import "@mantine/core/styles.css";
import { ReactFlowProvider } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useEffect, useRef, useState } from "react";

import { init, WebLogicVM } from "mindy-website";

import LogicVMFlow from "./components/LogicVMFlow";
import "./global.css";
import { theme } from "./theme";

export default function App() {
    const [vm, setVM] = useState<WebLogicVM>();
    const frameRef = useRef(0);

    useEffect(() => {
        init();

        const newVM = new WebLogicVM();
        setVM(newVM);

        // start tick loop
        const callback = (time: number) => {
            newVM.do_tick(time);
            frameRef.current = requestAnimationFrame(callback);
        };
        frameRef.current = requestAnimationFrame(callback);

        return () => {
            cancelAnimationFrame(frameRef.current);
        };
    }, []);

    return (
        <MantineProvider theme={theme} defaultColorScheme="dark">
            <ReactFlowProvider>
                <Center h="100vh">
                    {vm == null ? (
                        <Loader color="indigo" size="lg" />
                    ) : (
                        <LogicVMFlow vm={vm} />
                    )}
                </Center>
            </ReactFlowProvider>
        </MantineProvider>
    );
}
