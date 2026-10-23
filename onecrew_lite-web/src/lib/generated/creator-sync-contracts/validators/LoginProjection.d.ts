import type { LoginProjection } from "../types/LoginProjection";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateLoginProjection: ContractValidator<LoginProjection>;
