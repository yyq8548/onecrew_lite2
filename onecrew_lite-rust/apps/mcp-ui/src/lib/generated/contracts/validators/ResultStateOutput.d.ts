import type { ResultStateOutput } from "../types/ResultStateOutput";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateResultStateOutput: ContractValidator<ResultStateOutput>;
