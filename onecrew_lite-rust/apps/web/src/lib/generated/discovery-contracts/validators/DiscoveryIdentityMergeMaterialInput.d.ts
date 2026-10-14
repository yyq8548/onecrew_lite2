import type { DiscoveryIdentityMergeMaterialInput } from "../types/DiscoveryIdentityMergeMaterialInput";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateDiscoveryIdentityMergeMaterialInput: ContractValidator<DiscoveryIdentityMergeMaterialInput>;
