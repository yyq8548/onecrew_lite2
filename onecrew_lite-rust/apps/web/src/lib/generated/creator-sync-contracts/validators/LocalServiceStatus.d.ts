import type { LocalServiceStatus } from "../types/LocalServiceStatus";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateLocalServiceStatus: ContractValidator<LocalServiceStatus>;
