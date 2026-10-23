import type { ExactResultStateInput } from "../types/ExactResultStateInput";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateExactResultStateInput: ContractValidator<ExactResultStateInput>;
