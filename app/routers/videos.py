from typing import Dict, Any, Optional

from fastapi import APIRouter, Depends, HTTPException, status, Response, Request
from fastapi.responses import StreamingResponse

from app.core.firebase_client import get_current_user
from app.core import saas, storage
from app.core.security import ValidationError, validate_video_id, validate_clip_name
from app.schemas import (
    VideoInfoResponse,
    UserVideosResponse,
    DeleteVideoResponse,
    BulkDeleteVideosRequest,
    BulkDeleteVideosResponse,
    DeleteClipResponse,
)

router = APIRouter(prefix="/api", tags=["Videos"])

@router.get("/videos/{video_id}", response_model=VideoInfoResponse)
async def get_video_info(
    video_id: str,
    user: Dict[str, Any] = Depends(get_current_user),
) -> VideoInfoResponse:
    """Get video information and clips."""
    # Validate video_id to prevent path traversal
    try:
        video_id = validate_video_id(video_id)
    except ValidationError as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=e.message)
    
    uid = user["uid"]
    from app.config import logger

    # Check ownership in DB first
    is_owner = saas.user_owns_video(uid, video_id)
    
    # Load highlights metadata (from storage)
    highlights_data = storage.load_highlights(uid, video_id)
    
    logger.info(f"Debug get_video_info: uid={uid} video_id={video_id} is_owner={is_owner} highlights_found={bool(highlights_data)}")

    # If not in DB and not in storage, then it truly doesn't exist (or isn't yours)
    if not is_owner and not highlights_data:
        logger.warning(f"Video not found for user {uid}: {video_id}")
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Video not found")
    
    highlights_map: Dict[int, Dict[str, str]] = {}
    for h in highlights_data.get("highlights", []):
        h_id = h.get("id")
        if h_id is not None:
            highlights_map[int(h_id)] = {
                "title": h.get("title", ""),
                "description": h.get("description", ""),
            }
    
    clips = storage.list_clips_with_metadata(uid, video_id, highlights_map)
    
    return VideoInfoResponse(
        id=video_id,
        clips=clips,
        custom_prompt=highlights_data.get("custom_prompt"),
    )


@router.get("/user/videos", response_model=UserVideosResponse)
async def get_user_videos(
    user: Dict[str, Any] = Depends(get_current_user),
) -> UserVideosResponse:
    """List all videos for the authenticated user."""
    uid = user["uid"]
    videos = saas.list_user_videos(uid)
    return UserVideosResponse(videos=videos)


@router.get("/videos/{video_id}/clips/{clip_name}")
async def get_clip(
    video_id: str,
    clip_name: str,
    request: Request,
    user: Dict[str, Any] = Depends(get_current_user),
) -> Response:
    """Stream a video clip file with support for HTTP range requests."""
    from app.config import logger
    
    # Validate video_id and clip_name to prevent path traversal
    try:
        video_id = validate_video_id(video_id)
    except ValidationError as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=e.message)
    
    # Validate clip_name - should only contain safe characters
    if not clip_name or ".." in clip_name or "/" in clip_name or "\\" in clip_name:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Invalid clip name")
    
    uid = user["uid"]
    
    # Check ownership
    is_owner = saas.user_owns_video(uid, video_id)
    highlights_data = storage.load_highlights(uid, video_id)
    
    if not is_owner and not highlights_data:
        logger.warning(f"Video not found for user {uid}: {video_id}")
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Video not found")
    
    # Construct the R2 key
    key = f"{uid}/{video_id}/clips/{clip_name}"
    
    # Get the object from R2
    obj = storage.get_object(key)
    if not obj:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Clip not found")
    
    # Get file size
    content_length = obj.get("ContentLength", 0)
    
    # Determine content type
    content_type = obj.get("ContentType", "video/mp4")
    if not content_type.startswith("video/") and not content_type.startswith("image/"):
        # Default to video/mp4 for .mp4 files, image/jpeg for .jpg files
        if clip_name.lower().endswith(".mp4"):
            content_type = "video/mp4"
        elif clip_name.lower().endswith(".jpg") or clip_name.lower().endswith(".jpeg"):
            content_type = "image/jpeg"
    
    # Handle range requests for video seeking
    range_header = request.headers.get("range")
    if range_header:
        # Parse range header (e.g., "bytes=0-1023")
        try:
            range_match = range_header.replace("bytes=", "").split("-")
            start = int(range_match[0]) if range_match[0] else 0
            end = int(range_match[1]) if range_match[1] else content_length - 1
            
            if start < 0 or end >= content_length or start > end:
                raise HTTPException(
                    status_code=status.HTTP_416_REQUESTED_RANGE_NOT_SATISFIABLE,
                    detail="Invalid range",
                    headers={"Content-Range": f"bytes */{content_length}"}
                )
            
            # Get range from R2
            range_obj = storage.get_object(key, Range=f"bytes={start}-{end}")
            if not range_obj:
                raise HTTPException(status_code=status.HTTP_416_REQUESTED_RANGE_NOT_SATISFIABLE)
            
            def generate_range():
                body = range_obj["Body"]
                while True:
                    chunk = body.read(8192)
                    if not chunk:
                        break
                    yield chunk
            
            content_range = f"bytes {start}-{end}/{content_length}"
            return StreamingResponse(
                generate_range(),
                status_code=206,  # Partial Content
                media_type=content_type,
                headers={
                    "Content-Range": content_range,
                    "Accept-Ranges": "bytes",
                    "Content-Length": str(end - start + 1),
                    "Cache-Control": "public, max-age=3600",
                    "Cross-Origin-Resource-Policy": "cross-origin",
                }
            )
        except (ValueError, IndexError):
            # Invalid range header, fall through to full file
            pass
    
    # Stream the full file
    def generate():
        body = obj["Body"]
        while True:
            chunk = body.read(8192)  # Read in 8KB chunks
            if not chunk:
                break
            yield chunk
    
    return StreamingResponse(
        generate(),
        media_type=content_type,
        headers={
            "Accept-Ranges": "bytes",
            "Content-Length": str(content_length),
            "Cache-Control": "public, max-age=3600",
            "Cross-Origin-Resource-Policy": "cross-origin",
        }
    )


@router.delete("/videos/{video_id}", response_model=DeleteVideoResponse)
async def delete_video(
    video_id: str,
    user: Dict[str, Any] = Depends(get_current_user),
) -> DeleteVideoResponse:
    """
    Delete a single video and all associated files.
    
    This endpoint:
    1. Validates ownership
    2. Deletes all files from R2 storage
    3. Deletes the video record from Firestore
    
    Returns 404 if video doesn't exist or user doesn't own it.
    """
    from app.config import logger
    
    # Validate video_id to prevent path traversal
    try:
        video_id = validate_video_id(video_id)
    except ValidationError as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=e.message)
    
    uid = user["uid"]
    
    # Verify ownership before deletion
    is_owner = saas.user_owns_video(uid, video_id)
    if not is_owner:
        logger.warning(f"User {uid} attempted to delete video {video_id} they don't own")
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="Video not found"
        )
    
    try:
        # Delete files from R2 storage
        files_deleted = 0
        try:
            files_deleted = storage.delete_video_files(uid, video_id)
        except Exception as e:
            logger.error(f"Failed to delete files for video {video_id}: {e}")
            # Continue with DB deletion even if file deletion fails
        
        # Delete record from Firestore
        deleted = saas.delete_video(uid, video_id)
        if not deleted:
            logger.warning(f"Video {video_id} not found in database during deletion")
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Video not found"
            )
        
        logger.info(f"Successfully deleted video {video_id} for user {uid} ({files_deleted} files)")
        
        return DeleteVideoResponse(
            success=True,
            video_id=video_id,
            message="Video deleted successfully",
            files_deleted=files_deleted,
        )
    except HTTPException:
        raise
    except Exception as e:
        logger.error(f"Error deleting video {video_id}: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail="Failed to delete video"
        )


@router.delete("/videos", response_model=BulkDeleteVideosResponse)
async def bulk_delete_videos(
    request: BulkDeleteVideosRequest,
    user: Dict[str, Any] = Depends(get_current_user),
) -> BulkDeleteVideosResponse:
    """
    Delete multiple videos and all associated files.
    
    This endpoint:
    1. Validates ownership for each video
    2. Deletes all files from R2 storage for each video
    3. Deletes video records from Firestore
    
    Returns detailed results for each video_id indicating success/failure.
    """
    from app.config import logger
    
    uid = user["uid"]
    video_ids = request.video_ids
    
    # Verify ownership for all videos before proceeding
    owned_video_ids = []
    for video_id in video_ids:
        if saas.user_owns_video(uid, video_id):
            owned_video_ids.append(video_id)
        else:
            logger.warning(f"User {uid} attempted to delete video {video_id} they don't own")
    
    if not owned_video_ids:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="None of the specified videos belong to the current user"
        )
    
    results: Dict[str, Dict[str, Any]] = {}
    
    # Initialize results for all video_ids
    for video_id in video_ids:
        if video_id not in owned_video_ids:
            results[video_id] = {
                "success": False,
                "error": "Video not found or access denied"
            }
    
    # Delete files from R2 storage for owned videos (must be done individually per video)
    files_deleted_map: Dict[str, int] = {}
    for video_id in owned_video_ids:
        try:
            files_deleted = storage.delete_video_files(uid, video_id)
            files_deleted_map[video_id] = files_deleted
        except Exception as e:
            logger.error(f"Failed to delete files for video {video_id}: {e}")
            files_deleted_map[video_id] = 0
            # Continue with DB deletion even if file deletion fails
    
    # Batch delete records from Firestore (more efficient)
    db_results = saas.delete_videos(uid, owned_video_ids)
    
    # Combine results
    deleted_count = 0
    failed_count = 0
    
    for video_id in video_ids:
        if video_id not in owned_video_ids:
            failed_count += 1
            continue
        
        db_success = db_results.get(video_id, False)
        files_deleted = files_deleted_map.get(video_id, 0)
        
        if db_success:
            results[video_id] = {
                "success": True,
                "files_deleted": files_deleted,
            }
            deleted_count += 1
            logger.info(f"Successfully deleted video {video_id} for user {uid} ({files_deleted} files)")
        else:
            results[video_id] = {
                "success": False,
                "error": "Video not found in database",
                "files_deleted": files_deleted,  # Files were deleted even if DB record wasn't found
            }
            failed_count += 1
    
    return BulkDeleteVideosResponse(
        success=deleted_count > 0,
        deleted_count=deleted_count,
        failed_count=failed_count,
        results=results,
    )


@router.delete("/videos/{video_id}/clips/{clip_name}", response_model=DeleteClipResponse)
async def delete_clip(
    video_id: str,
    clip_name: str,
    user: Dict[str, Any] = Depends(get_current_user),
) -> DeleteClipResponse:
    """
    Delete a single clip and its thumbnail.
    
    This endpoint:
    1. Validates video ownership
    2. Validates clip name format
    3. Deletes the clip file and thumbnail from R2 storage
    
    Returns 404 if video doesn't exist, user doesn't own it, or clip doesn't exist.
    """
    from app.config import logger
    
    # Validate video_id to prevent path traversal
    try:
        video_id = validate_video_id(video_id)
    except ValidationError as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=e.message)
    
    # Validate clip_name to prevent path traversal
    try:
        clip_name = validate_clip_name(clip_name)
    except ValidationError as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=e.message)
    
    uid = user["uid"]
    
    # Verify ownership before deletion
    is_owner = saas.user_owns_video(uid, video_id)
    highlights_data = storage.load_highlights(uid, video_id)
    
    if not is_owner and not highlights_data:
        logger.warning(f"User {uid} attempted to delete clip from video {video_id} they don't own")
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="Video not found"
        )
    
    try:
        # Delete clip and thumbnail from R2 storage
        files_deleted = 0
        try:
            files_deleted = storage.delete_clip(uid, video_id, clip_name)
        except ValueError as e:
            # Invalid clip name format
            raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=str(e))
        except Exception as e:
            logger.error(f"Failed to delete clip {clip_name} for video {video_id}: {e}")
            raise HTTPException(
                status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
                detail="Failed to delete clip"
            )
        
        if files_deleted == 0:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail="Clip not found"
            )
        
        logger.info(f"Successfully deleted clip {clip_name} from video {video_id} for user {uid} ({files_deleted} files)")
        
        return DeleteClipResponse(
            success=True,
            video_id=video_id,
            clip_name=clip_name,
            message="Clip deleted successfully",
            files_deleted=files_deleted,
        )
    except HTTPException:
        raise
    except Exception as e:
        logger.error(f"Error deleting clip {clip_name} from video {video_id}: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail="Failed to delete clip"
        )
