import {
    AppShell,
    Center,
    Flex,
    Loader,
    MantineProvider,
    Text,
} from "@mantine/core";
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
            <ReactFlowProvider>
                <Suspense
                    fallback={
                        <Center pt="50vh">
                            <Loader color="indigo" size="lg" />
                        </Center>
                    }
                >
                    <AppShell h="100vh" footer={{ height: 30 }}>
                        <AppShell.Main h="100%">
                            <LogicVMProvider>
                                <LogicVMFlow />
                            </LogicVMProvider>
                        </AppShell.Main>

                        <AppShell.Footer>
                            <Flex h="100%" justify="center" align="center">
                                <Text
                                    size="sm"
                                    c="dimmed"
                                    td="underline"
                                    component="a"
                                    href="https://github.com/object-Object/mindy/tree/main/mindy-website"
                                    target="_blank"
                                    rel="noopener noreferrer"
                                >
                                    Source
                                </Text>
                            </Flex>
                        </AppShell.Footer>
                    </AppShell>
                </Suspense>
            </ReactFlowProvider>
        </MantineProvider>
    );
}
