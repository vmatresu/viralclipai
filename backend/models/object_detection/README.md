# Object Detection Models

This directory contains ONNX models for object detection.

## YOLOv8n

The YOLOv8n model file (`yolov8n.onnx`) is not included in the repository due to its size.

### To download the model:

**Option 1: Export using Python (recommended)**

```bash
pip install ultralytics
python -c "from ultralytics import YOLO; model = YOLO('yolov8n.pt'); model.export(format='onnx')"
mv yolov8n.onnx backend/models/object_detection/
```

**Option 2: Manual download**

Download from the Ultralytics releases page and place in this directory.

## Model Info

| Model | Size | Input | Classes |
|-------|------|-------|---------|
| yolov8n.onnx | ~6MB | 640x640 | 80 COCO classes |

## COCO Classes

The model detects standard COCO objects including:
- person (0)
- car, bus, truck (vehicles)
- chair, couch (furniture)
- etc.
