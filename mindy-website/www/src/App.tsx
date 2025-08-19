import { Center, Loader, MantineProvider } from "@mantine/core";
import "@mantine/core/styles.css";
import { ReactFlowProvider } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Suspense } from "react";

import LogicVMFlow from "./components/LogicVMFlow";
import LogicVMProvider from "./components/LogicVMProvider";
import "./global.css";
import { theme } from "./theme";

export default function App() {
    return (
        <MantineProvider theme={theme} defaultColorScheme="dark">
            <Center h="100vh">
                <Suspense fallback={<Loader color="indigo" size="lg" />}>
                    <LogicVMProvider>
                        <ReactFlowProvider>
                            <LogicVMFlow />
                        </ReactFlowProvider>
                    </LogicVMProvider>
                </Suspense>
            </Center>
        </MantineProvider>
    );
}
