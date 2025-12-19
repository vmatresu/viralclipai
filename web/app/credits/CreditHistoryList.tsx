"use client";

import {
  AlertCircle,
  Clock,
  CreditCard,
  ExternalLink,
  Loader2,
  TrendingUp,
  Zap,
} from "lucide-react";
import Link from "next/link";
import { useCallback, useEffect, useRef, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  getCreditHistory,
  type CreditHistoryResponse,
  type CreditOperationType,
  type CreditTransaction,
} from "@/lib/apiClient";
import { useAuth } from "@/lib/auth";

// Format large numbers with commas
function formatNumber(num: number): string {
  return num.toLocaleString();
}

// Format date for display
function formatDate(isoString: string): string {
  const date = new Date(isoString);
  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

// Known operation types for type-safe label lookup
const OPERATION_LABELS: Record<CreditOperationType, string> = {
  analysis: "Video Analysis",
  scene_processing: "Scene Processing",
  reprocessing: "Reprocessing",
  silent_remover: "Silent Remover",
  object_detection: "Object Detection",
  scene_originals: "Scene Originals",
  generate_more_scenes: "Generate More Scenes",
  admin_adjustment: "Admin Adjustment",
};

// Fix #10: Type-safe operation label with fallback for unknown types
function getOperationLabel(type: string): string {
  if (type in OPERATION_LABELS) {
    return OPERATION_LABELS[type as CreditOperationType];
  }
  // Fallback: format unknown types nicely (e.g., "some_type" -> "Some Type")
  return type
    .split("_")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

// Get operation type badge color
// Fix #10: Accept string type for badge variant (handles unknown types)
function getOperationBadgeVariant(type: string): "default" | "secondary" | "outline" {
  switch (type) {
    case "analysis":
      return "secondary";
    case "scene_processing":
    case "reprocessing":
      return "default";
    case "admin_adjustment":
      return "outline";
    default:
      return "secondary";
  }
}

interface CreditSummaryCardProps {
  summary: CreditHistoryResponse["summary"] | null;
  loading: boolean;
}

function CreditSummaryCard({ summary, loading }: CreditSummaryCardProps) {
  if (loading) {
    return (
      <Card className="glass">
        <CardHeader className="pb-2">
          <CardTitle className="text-lg flex items-center gap-2">
            <Zap className="h-5 w-5 text-primary" />
            Credit Usage
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            Loading usage information...
          </div>
        </CardContent>
      </Card>
    );
  }

  if (!summary) {
    return (
      <Card className="glass">
        <CardHeader className="pb-2">
          <CardTitle className="text-lg flex items-center gap-2">
            <Zap className="h-5 w-5 text-primary" />
            Credit Usage
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="text-sm text-muted-foreground">
            Unable to load usage information.
          </div>
        </CardContent>
      </Card>
    );
  }

  // Fix #9: Guard against division by zero when monthly_limit is 0
  const usagePercentage =
    summary.monthly_limit > 0
      ? Math.min((summary.total_used / summary.monthly_limit) * 100, 100)
      : 0;
  const isHighUsage = usagePercentage >= 80;
  const isNearLimit = usagePercentage >= 90;

  const getProgressBarColor = () => {
    if (isNearLimit) return "bg-destructive";
    if (isHighUsage) return "bg-destructive/80";
    return "bg-primary";
  };

  return (
    <Card className="glass">
      <CardHeader className="pb-2">
        <CardTitle className="text-lg flex items-center gap-2">
          <Zap className="h-5 w-5 text-primary" />
          Credit Usage
        </CardTitle>
        <CardDescription>{summary.month} billing period</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Monthly Credits Usage */}
        <div className="space-y-2">
          <div className="flex justify-between text-sm">
            <span className="text-muted-foreground">Monthly Credits</span>
            <span
              className={
                isHighUsage ? "text-destructive font-semibold" : "text-muted-foreground"
              }
            >
              {formatNumber(summary.total_used)} / {formatNumber(summary.monthly_limit)}
            </span>
          </div>
          <div className="relative h-3 w-full overflow-hidden rounded-full bg-muted">
            <div
              className={`h-full transition-all duration-500 ${getProgressBarColor()}`}
              style={{ width: `${usagePercentage}%` }}
            />
          </div>
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span className="flex items-center gap-1">
              <TrendingUp className="h-3 w-3" />
              {formatNumber(summary.remaining)} credits remaining
            </span>
            {isHighUsage && (
              <span className="text-destructive">
                {isNearLimit ? "Almost at limit!" : "High usage"}
              </span>
            )}
          </div>
        </div>

        {/* Usage by Operation */}
        {Object.keys(summary.by_operation).length > 0 && (
          <div className="space-y-2 pt-2 border-t border-muted">
            <div className="text-sm text-muted-foreground">Breakdown by operation:</div>
            <div className="grid grid-cols-2 gap-2">
              {Object.entries(summary.by_operation).map(([type, amount]) => (
                <div key={type} className="flex justify-between text-xs">
                  <span className="text-muted-foreground">
                    {getOperationLabel(type)}
                  </span>
                  <span className="font-medium">{formatNumber(amount)}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

interface TransactionRowProps {
  transaction: CreditTransaction;
}

// Format cost breakdown from metadata
function getCostBreakdown(transaction: CreditTransaction): string | null {
  const metadata = transaction.metadata;
  if (!metadata) return null;

  const parts: string[] = [];

  // Per-style breakdown: "streamer_split:10,original:10" -> "streamer_split: 10, original: 10"
  const styleBreakdown = metadata.style_breakdown;
  if (styleBreakdown) {
    const styleParts = styleBreakdown.split(",").map((s) => {
      const [name, cost] = s.split(":");
      return `${name}: ${cost}`;
    });
    parts.push(styleParts.join(", "));
  } else {
    // Fallback to old format
    const styleCredits = metadata.style_credits;
    const styles = metadata.styles;
    if (styleCredits && styles) {
      parts.push(`${styles}: ${styleCredits}`);
    }
  }

  // Silent remover addon
  const silentRemoverCredits = metadata.silent_remover_credits;
  if (silentRemoverCredits && parseInt(silentRemoverCredits, 10) > 0) {
    parts.push(`Silent remover: ${silentRemoverCredits}`);
  }

  // Object detection addon
  const objectDetectionCredits = metadata.object_detection_credits;
  if (objectDetectionCredits && parseInt(objectDetectionCredits, 10) > 0) {
    parts.push(`Object detection: ${objectDetectionCredits}`);
  }

  if (parts.length === 0) return null;
  return parts.join(" + ");
}

function TransactionRow({ transaction }: TransactionRowProps) {
  // For analysis transactions, we may have source_url in metadata instead of video_id
  const sourceUrl = transaction.metadata?.source_url;
  const hasVideoLink = Boolean(transaction.video_id);
  const hasDraftLink = !hasVideoLink && Boolean(transaction.draft_id);
  const hasSourceLink = !hasVideoLink && !hasDraftLink && Boolean(sourceUrl);

  // Get cost breakdown for display
  const costBreakdown = getCostBreakdown(transaction);

  return (
    <TableRow>
      <TableCell className="text-muted-foreground text-sm">
        {formatDate(transaction.timestamp)}
      </TableCell>
      <TableCell>
        <Badge variant={getOperationBadgeVariant(transaction.operation_type)}>
          {getOperationLabel(transaction.operation_type)}
        </Badge>
      </TableCell>
      <TableCell>
        <div>{transaction.description}</div>
        {costBreakdown && (
          <div className="text-xs text-muted-foreground mt-0.5">{costBreakdown}</div>
        )}
      </TableCell>
      <TableCell className="text-right font-medium">
        -{formatNumber(transaction.credits_amount)}
      </TableCell>
      <TableCell className="text-right text-muted-foreground">
        {formatNumber(transaction.balance_after)}
      </TableCell>
      <TableCell>
        {hasVideoLink && (
          <Link
            href={`/history/${transaction.video_id}`}
            className="text-xs text-primary hover:underline"
          >
            View video
          </Link>
        )}
        {hasDraftLink && (
          <Link
            href={`/drafts/${transaction.draft_id}`}
            className="text-xs text-primary hover:underline"
          >
            View draft
          </Link>
        )}
        {hasSourceLink && (
          <a
            href={sourceUrl}
            target="_blank"
            rel="noopener noreferrer"
            className="text-xs text-primary hover:underline inline-flex items-center gap-1"
          >
            Source <ExternalLink className="h-3 w-3" />
          </a>
        )}
      </TableCell>
    </TableRow>
  );
}

export default function CreditHistoryList() {
  const { user, loading: authLoading, getIdToken } = useAuth();
  const [data, setData] = useState<CreditHistoryResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [loadingMore, setLoadingMore] = useState(false);

  const fetchHistory = useCallback(
    async (pageToken?: string) => {
      const token = await getIdToken();
      if (!token) {
        setError("Please sign in to view credit history");
        setLoading(false);
        return;
      }

      try {
        const isLoadingMore = Boolean(pageToken);
        if (isLoadingMore) {
          setLoadingMore(true);
        } else {
          setLoading(true);
        }

        const response = await getCreditHistory(token, {
          limit: 50,
          cursor: pageToken,
        });

        if (isLoadingMore && data) {
          // Append to existing transactions
          setData({
            ...response,
            transactions: [...data.transactions, ...response.transactions],
          });
        } else {
          setData(response);
        }
        setError(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to load credit history");
      } finally {
        setLoading(false);
        setLoadingMore(false);
      }
    },
    [getIdToken, data]
  );

  // Fix #8: Properly include fetchHistory in deps with a ref to prevent infinite loops
  const fetchHistoryRef = useRef(fetchHistory);
  fetchHistoryRef.current = fetchHistory;

  useEffect(() => {
    if (!authLoading && user) {
      void fetchHistoryRef.current();
    } else if (!authLoading && !user) {
      setLoading(false);
      setError("Please sign in to view credit history");
    }
  }, [authLoading, user]);

  if (authLoading || loading) {
    return (
      <div className="container mx-auto py-8 px-4 max-w-6xl">
        <div className="flex flex-col items-center justify-center py-24 space-y-4">
          <Loader2 className="h-8 w-8 animate-spin text-primary" />
          <p className="text-muted-foreground">Loading credit history...</p>
        </div>
      </div>
    );
  }

  if (!user) {
    return (
      <div className="container mx-auto py-8 px-4 max-w-6xl">
        <Card className="glass">
          <CardContent className="py-12">
            <div className="flex flex-col items-center justify-center space-y-4 text-center">
              <CreditCard className="h-12 w-12 text-muted-foreground" />
              <h3 className="text-lg font-medium">Sign in to view credit history</h3>
              <p className="text-sm text-muted-foreground max-w-sm">
                Track your credit usage and see detailed transaction history.
              </p>
            </div>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="container mx-auto py-8 px-4 max-w-6xl">
      <div className="flex items-center gap-3 mb-6">
        <CreditCard className="h-7 w-7 text-primary" />
        <div>
          <h1 className="text-2xl font-bold">Credit History</h1>
          <p className="text-muted-foreground text-sm">
            Track your credit usage and transaction history
          </p>
        </div>
      </div>

      {error && (
        <Card className="glass mb-6 border-destructive/50">
          <CardContent className="py-4">
            <div className="flex items-center gap-2 text-destructive">
              <AlertCircle className="h-4 w-4" />
              <span className="text-sm">{error}</span>
            </div>
          </CardContent>
        </Card>
      )}

      <div className="grid gap-6 lg:grid-cols-3">
        {/* Summary Card */}
        <div className="lg:col-span-1">
          <CreditSummaryCard summary={data?.summary ?? null} loading={loading} />
        </div>

        {/* Transaction List */}
        <div className="lg:col-span-2">
          <Card className="glass">
            <CardHeader>
              <CardTitle className="text-lg flex items-center gap-2">
                <Clock className="h-5 w-5" />
                Transaction History
              </CardTitle>
              <CardDescription>
                Detailed record of all credit transactions
              </CardDescription>
            </CardHeader>
            <CardContent>
              {data?.transactions && data.transactions.length > 0 ? (
                <>
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>Date</TableHead>
                        <TableHead>Operation</TableHead>
                        <TableHead>Description</TableHead>
                        <TableHead className="text-right">Credits</TableHead>
                        <TableHead className="text-right">Balance</TableHead>
                        <TableHead />
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {data.transactions.map((tx) => (
                        <TransactionRow key={tx.id} transaction={tx} />
                      ))}
                    </TableBody>
                  </Table>

                  {data.next_page_token && (
                    <div className="flex justify-center mt-4">
                      <Button
                        variant="outline"
                        onClick={() => void fetchHistory(data.next_page_token)}
                        disabled={loadingMore}
                      >
                        {loadingMore ? (
                          <>
                            <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                            Loading...
                          </>
                        ) : (
                          "Load more"
                        )}
                      </Button>
                    </div>
                  )}
                </>
              ) : (
                <div className="flex flex-col items-center justify-center py-12 space-y-4 text-center">
                  <Clock className="h-12 w-12 text-muted-foreground/50" />
                  <h3 className="text-lg font-medium">No transactions yet</h3>
                  <p className="text-sm text-muted-foreground max-w-sm">
                    Credit transactions will appear here once you start using the
                    service.
                  </p>
                  <Button asChild variant="outline">
                    <Link href="/analyze">Analyze your first video</Link>
                  </Button>
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
