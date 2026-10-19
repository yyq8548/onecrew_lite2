import type { DiscoveryIdentityMergeMaterial } from "../types/DiscoveryIdentityMergeMaterial";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateDiscoveryIdentityMergeMaterial: ContractValidator<DiscoveryIdentityMergeMaterial>;
