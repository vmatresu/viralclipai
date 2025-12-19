import { useCallback, useEffect, useState } from "react";

// ============================================================================
// Types
// ============================================================================

export type SortField = "title" | "status" | "size" | "date";
export type SortDirection = "asc" | "desc";

export interface SortConfig {
  field: SortField;
  direction: SortDirection;
}

export interface PaginationState {
  currentPage: number;
  pageTokens: (string | null)[];
  nextPageToken: string | null;
}

export interface UseSortedPaginationOptions<T> {
  /** Function to fetch data with sort and pagination params */
  fetchFn: (params: {
    pageToken: string | null;
    sortField: SortField;
    sortDirection: SortDirection;
  }) => Promise<{ items: T[]; nextPageToken: string | null }>;
  /** Initial sort field */
  initialSortField?: SortField;
  /** Initial sort direction */
  initialSortDirection?: SortDirection;
  /** Whether to auto-fetch on mount */
  autoFetch?: boolean;
  /** Dependencies that should trigger a refetch */
  deps?: unknown[];
}

export interface UseSortedPaginationResult<T> {
  // Data
  items: T[];
  loading: boolean;
  error: string | null;

  // Sort state
  sortField: SortField;
  sortDirection: SortDirection;
  handleSort: (field: SortField) => void;

  // Pagination state
  currentPage: number;
  hasNextPage: boolean;
  hasPrevPage: boolean;
  handleNextPage: () => Promise<void>;
  handlePrevPage: () => Promise<void>;

  // Actions
  refresh: () => Promise<void>;
  setItems: React.Dispatch<React.SetStateAction<T[]>>;
}

// ============================================================================
// Hook Implementation
// ============================================================================

/**
 * Custom hook for server-side sorted pagination.
 *
 * Handles:
 * - Server-side sorting with automatic page reset on sort change
 * - Cursor-based pagination with back navigation support
 * - Loading and error states
 * - Automatic refetch on sort change
 *
 * @example
 * ```tsx
 * const {
 *   items,
 *   loading,
 *   sortField,
 *   sortDirection,
 *   handleSort,
 *   currentPage,
 *   hasNextPage,
 *   handleNextPage,
 *   handlePrevPage,
 * } = useSortedPagination({
 *   fetchFn: async ({ pageToken, sortField, sortDirection }) => {
 *     const data = await api.getItems({ pageToken, sortField, sortDirection });
 *     return { items: data.items, nextPageToken: data.nextToken };
 *   },
 *   initialSortField: "date",
 *   initialSortDirection: "desc",
 * });
 * ```
 */
export function useSortedPagination<T>({
  fetchFn,
  initialSortField = "date",
  initialSortDirection = "desc",
  autoFetch = true,
  deps = [],
}: UseSortedPaginationOptions<T>): UseSortedPaginationResult<T> {
  // Data state
  const [items, setItems] = useState<T[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Sort state
  const [sortField, setSortField] = useState<SortField>(initialSortField);
  const [sortDirection, setSortDirection] =
    useState<SortDirection>(initialSortDirection);

  // Pagination state
  const [pagination, setPagination] = useState<PaginationState>({
    currentPage: 0,
    pageTokens: [null],
    nextPageToken: null,
  });

  // Fetch function
  const fetchData = useCallback(
    async (pageToken: string | null, field: SortField, direction: SortDirection) => {
      setLoading(true);
      setError(null);

      try {
        const result = await fetchFn({
          pageToken,
          sortField: field,
          sortDirection: direction,
        });

        setItems(result.items);
        setPagination((prev) => ({
          ...prev,
          nextPageToken: result.nextPageToken,
        }));
      } catch (err) {
        const errorMessage = err instanceof Error ? err.message : "Failed to load data";
        setError(errorMessage);
      } finally {
        setLoading(false);
      }
    },
    [fetchFn]
  );

  // Handle sort change - resets pagination
  const handleSort = useCallback(
    (field: SortField) => {
      let newDirection: SortDirection;
      if (field === sortField) {
        newDirection = sortDirection === "asc" ? "desc" : "asc";
      } else {
        newDirection = "asc";
      }

      setSortField(field);
      setSortDirection(newDirection);

      // Reset pagination
      setPagination({
        currentPage: 0,
        pageTokens: [null],
        nextPageToken: null,
      });
    },
    [sortField, sortDirection]
  );

  // Handle next page
  const handleNextPage = useCallback(async () => {
    if (!pagination.nextPageToken) return;

    const nextPage = pagination.currentPage + 1;
    const newTokens = [...pagination.pageTokens, pagination.nextPageToken];

    setPagination((prev) => ({
      ...prev,
      currentPage: nextPage,
      pageTokens: newTokens,
    }));

    await fetchData(pagination.nextPageToken, sortField, sortDirection);
  }, [pagination, sortField, sortDirection, fetchData]);

  // Handle previous page
  const handlePrevPage = useCallback(async () => {
    if (pagination.currentPage === 0) return;

    const prevPage = pagination.currentPage - 1;
    const prevToken = pagination.pageTokens.at(prevPage) ?? null;

    setPagination((prev) => ({
      ...prev,
      currentPage: prevPage,
    }));

    await fetchData(prevToken, sortField, sortDirection);
  }, [pagination, sortField, sortDirection, fetchData]);

  // Refresh current page
  const refresh = useCallback(async () => {
    const currentToken = pagination.pageTokens[pagination.currentPage] ?? null;
    await fetchData(currentToken, sortField, sortDirection);
  }, [pagination, sortField, sortDirection, fetchData]);

  // Auto-fetch on sort change
  useEffect(() => {
    if (!autoFetch) return;
    void fetchData(null, sortField, sortDirection);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sortField, sortDirection, ...deps]);

  return {
    // Data
    items,
    loading,
    error,

    // Sort state
    sortField,
    sortDirection,
    handleSort,

    // Pagination state
    currentPage: pagination.currentPage,
    hasNextPage: Boolean(pagination.nextPageToken),
    hasPrevPage: pagination.currentPage > 0,
    handleNextPage,
    handlePrevPage,

    // Actions
    refresh,
    setItems,
  };
}
