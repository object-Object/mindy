import { createContext, use } from "react";

import type { VMWorkerType } from "./workers/vm";

export const LogicVMContext = createContext<VMWorkerType>(
    undefined as unknown as VMWorkerType,
);

export function useLogicVM() {
    const vm = use(LogicVMContext);
    if (vm == null) {
        throw new Error("Attempted to get the VM before it was initialized");
    }
    return vm;
}
