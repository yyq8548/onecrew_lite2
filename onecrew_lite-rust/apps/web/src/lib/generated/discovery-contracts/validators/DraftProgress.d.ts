import type { DraftProgress } from "../types/DraftProgress";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateDraftProgress: ContractValidator<DraftProgress>;
