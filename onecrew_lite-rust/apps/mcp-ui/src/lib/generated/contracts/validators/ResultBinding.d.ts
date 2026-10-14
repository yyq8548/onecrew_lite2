import type { ResultBinding } from "../types/ResultBinding";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateResultBinding: ContractValidator<ResultBinding>;
