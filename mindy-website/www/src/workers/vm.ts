// this is a separate file because otherwise firefox loads the worker twice
import type { DisplayKind, ProcessorKind } from "mindy-website";

// main thread -> worker

interface AddDisplayRequest {
    type: "addDisplay";
    position: number;
    kind: DisplayKind;
    width: number;
    height: number;
    canvas: OffscreenCanvas;
}

interface AddProcessorRequest {
    type: "addProcessor";
    position: number;
    kind: ProcessorKind;
}

interface SetProcessorCodeRequest {
    type: "setProcessorCode";
    position: number;
    code: string;
    links: Uint32Array;
}

interface RemoveBuildingRequest {
    type: "removeBuilding";
    position: number;
}

interface SetTargetFPSRequest {
    type: "setTargetFPS";
    target: number;
}

export type VMWorkerRequest =
    | AddProcessorRequest
    | AddDisplayRequest
    | SetProcessorCodeRequest
    | RemoveBuildingRequest
    | SetTargetFPSRequest;

// worker -> main thread

interface ReadyResponse {
    type: "ready";
}

interface BuildingAddedResponse {
    type: "buildingAdded";
    position: number;
    name?: string;
}

interface ProcessorCodeSetResponse {
    type: "processorCodeSet";
    position: number;
    links?: Map<number, string>;
    error?: string;
}

export type VMWorkerResponse =
    | ReadyResponse
    | BuildingAddedResponse
    | ProcessorCodeSetResponse;

// worker

export interface VMWorkerType
    extends Omit<Worker, "onmessage" | "postMessage"> {
    onmessage:
        | ((this: VMWorkerType, ev: MessageEvent<VMWorkerResponse>) => void)
        | null;

    postMessage(message: VMWorkerRequest, transfer?: Transferable[]): void;
}
