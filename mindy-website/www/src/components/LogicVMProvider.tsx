import { use, type ReactNode } from "react";

import init, { init_logging } from "mindy-website";

import { LogicVMContext } from "../hooks";
import type { VMWorkerResponse, VMWorkerType } from "../workers/vm";
import VMWorker from "../workers/vm.worker?worker";

const initPromise = init();

const vmPromise = new Promise<VMWorkerType>((resolve) => {
    const vm = new VMWorker({ name: "VMWorker" }) as VMWorkerType;

    const listener = ({ data }: MessageEvent<VMWorkerResponse>) => {
        if (data.type === "ready") {
            vm.removeEventListener("message", listener);
            resolve(vm);
        }
    };

    vm.addEventListener("message", listener);
});

interface LogicVMProviderProps {
    children: ReactNode;
}

export default function LogicVMProvider({ children }: LogicVMProviderProps) {
    use(initPromise);
    const vm = use(vmPromise);

    init_logging();

    return <LogicVMContext value={vm}>{children}</LogicVMContext>;
}
