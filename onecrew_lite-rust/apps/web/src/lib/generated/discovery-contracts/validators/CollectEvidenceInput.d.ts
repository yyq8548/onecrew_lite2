import type { CollectEvidenceInput } from "../types/CollectEvidenceInput";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateCollectEvidenceInput: ContractValidator<CollectEvidenceInput>;
