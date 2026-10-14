import type { ExecutionInput } from "../types/ExecutionInput";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateExecutionInput: ContractValidator<ExecutionInput>;
