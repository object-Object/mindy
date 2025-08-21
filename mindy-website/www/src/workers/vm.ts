// this is a separate file because otherwise firefox loads the worker twice
import type { DisplayKind, MemoryKind, ProcessorKind } from "mindy-website";

// main thread -> worker

interface AddDisplayRequest {
    type: "addDisplay";
    position: number;
    kind: DisplayKind;
    width: number;
    height: number;
    canvas: OffscreenCanvas;
}

interface AddMemoryRequest {
    type: "addMemory";
    position: number;
    kind: MemoryKind;
}

interface AddMessageRequest {
    type: "addMessage";
    position: number;
}

interface AddProcessorRequest {
    type: "addProcessor";
    position: number;
    kind: ProcessorKind;
}

interface AddSwitchRequest {
    type: "addSwitch";
    position: number;
}

interface SetMessageTextRequest {
    type: "setMessageText";
    position: number;
    value: string;
}

interface SetProcessorCodeRequest {
    type: "setProcessorCode";
    position: number;
    code: string;
    links: Uint32Array;
}

interface SetSwitchEnabledRequest {
    type: "setSwitchEnabled";
    position: number;
    value: boolean;
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
    | AddDisplayRequest
    | AddMemoryRequest
    | AddMessageRequest
    | AddProcessorRequest
    | AddSwitchRequest
    | SetMessageTextRequest
    | SetProcessorCodeRequest
    | SetSwitchEnabledRequest
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
    message: string;
    processor: { links?: Map<number, string>; error?: string };
    switch: boolean;
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
