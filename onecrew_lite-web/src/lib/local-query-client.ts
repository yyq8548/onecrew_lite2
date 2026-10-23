import { QueryClient } from '@tanstack/react-query';

// 网页只访问本地服务；互联网是否可用由服务端的实际请求判定。
export const createLocalQueryClient = () => new QueryClient({
  defaultOptions: {
    queries: { networkMode: 'always' },
    mutations: { networkMode: 'always' },
  },
});
