import type { ExecutionStatus } from "../types/ExecutionStatus";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateExecutionStatus: ContractValidator<ExecutionStatus>;
