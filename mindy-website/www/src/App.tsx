import { Center, MantineProvider, Paper } from "@mantine/core";
import "@mantine/core/styles.css";
import { useEffect, useRef } from "react";

import {
    init,
    PackedPoint2,
    ProcessorKind,
    WebLogicVMBuilder,
} from "mindy-website";

import classes from "./App.module.css";
import { theme } from "./theme";

function App() {
    const displayRef = useRef<HTMLDivElement>(null);
    const frameRef = useRef(0);

    useEffect(() => {
        init();

        // if this isn't here, it adds multiple canvases in dev mode for some reason
        displayRef.current!.replaceChildren();

        const builder = new WebLogicVMBuilder();
        builder.add_processor(new PackedPoint2(0, 0), ProcessorKind.World);
        builder.add_display(
            new PackedPoint2(1, 0),
            256,
            256,
            displayRef.current!,
        );
        const vm = builder.build();

        vm.set_processor_config(
            new PackedPoint2(0, 0),
            `
            draw clear 0 0 0
            drawflush display1
            stop
            `,
            [new PackedPoint2(1, 0)],
        );

        // start tick loop

        const callback = (time: number) => {
            vm.do_tick(time);
            frameRef.current = requestAnimationFrame(callback);
        };
        frameRef.current = requestAnimationFrame(callback);

        return () => {
            cancelAnimationFrame(frameRef.current);
        };
    }, []);

    return (
        <MantineProvider theme={theme} defaultColorScheme="dark">
            <Center pt={16}>
                <Paper
                    withBorder
                    w="fit-content"
                    h="fit-content"
                    className={classes.display}
                    ref={displayRef}
                ></Paper>
            </Center>
        </MantineProvider>
    );
}

export default App;
