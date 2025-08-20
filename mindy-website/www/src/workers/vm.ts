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

export interface BuildingUpdateMap {
    processor: { links?: Map<number, string>; error?: string };
}

interface BuildingUpdatedResponse<K extends keyof BuildingUpdateMap> {
    type: "buildingUpdated";
    position: number;
    buildingType: K;
    update: BuildingUpdateMap[K];
}

export type VMWorkerResponse =
    | ReadyResponse
    | BuildingAddedResponse
    | BuildingUpdatedResponse<keyof BuildingUpdateMap>;

// worker

interface VMWorkerEventMap extends WorkerEventMap {
    message: MessageEvent<VMWorkerResponse>;
    messageerror: MessageEvent<VMWorkerResponse>;
}

export interface VMWorkerType extends Worker {
    onmessage:
        | ((this: VMWorkerType, ev: MessageEvent<VMWorkerResponse>) => unknown)
        | null;
    onmessageerror:
        | ((this: VMWorkerType, ev: MessageEvent<VMWorkerResponse>) => unknown)
        | null;

    postMessage(message: VMWorkerRequest, transfer: Transferable[]): void;
    postMessage(
        message: VMWorkerRequest,
        options?: StructuredSerializeOptions,
    ): void;

    addEventListener<K extends keyof VMWorkerEventMap>(
        type: K,
        listener: (this: VMWorkerType, ev: VMWorkerEventMap[K]) => unknown,
        options?: boolean | AddEventListenerOptions,
    ): void;
    addEventListener(
        type: string,
        listener: EventListenerOrEventListenerObject,
        options?: boolean | AddEventListenerOptions,
    ): void;
}
