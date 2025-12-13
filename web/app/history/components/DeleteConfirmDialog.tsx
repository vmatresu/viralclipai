"use client";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

import { type DeleteTarget } from "../types";

interface DeleteConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  target: DeleteTarget | null;
  deleting: boolean;
  selectedCount: number;
  totalCount: number;
  onConfirm: () => void;
}

export function DeleteConfirmDialog({
  open,
  onOpenChange,
  target,
  deleting,
  selectedCount,
  totalCount,
  onConfirm,
}: DeleteConfirmDialogProps) {
  const getDialogContent = () => {
    if (!target) return { title: "", description: "" };

    if (target.type === "single") {
      return {
        title: "Delete Video",
        description:
          "Are you sure you want to delete this video? This action cannot be undone and will delete all associated clips and files.",
      };
    }
    if (target.type === "bulk") {
      return {
        title: `Delete ${selectedCount} Video${selectedCount > 1 ? "s" : ""}`,
        description: `Are you sure you want to delete ${selectedCount} selected video${selectedCount > 1 ? "s" : ""}? This action cannot be undone and will delete all associated clips and files.`,
      };
    }
    return {
      title: `Delete All Videos (${totalCount})`,
      description: `Are you sure you want to delete all ${totalCount} videos? This action cannot be undone and will delete all associated clips and files.`,
    };
  };

  const { title, description } = getDialogContent();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={deleting}
          >
            Cancel
          </Button>
          <Button variant="destructive" onClick={onConfirm} disabled={deleting}>
            {deleting ? (
              <>
                <span className="h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent mr-2" />
                Deleting...
              </>
            ) : (
              "Delete"
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
