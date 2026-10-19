export type CatalogShape<T> = {
  [Key in keyof T]: T[Key] extends string ? string : CatalogShape<T[Key]>;
};
