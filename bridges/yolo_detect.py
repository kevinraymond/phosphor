#!/usr/bin/env python3
"""
Phosphor Bridge — YOLO Object Detection

Detects objects via webcam and streams bounding box data + counts
to Phosphor's binding bus. Uses ultralytics YOLOv8.

Install:
    pip install ultralytics opencv-python websocket-client

Run:
    python yolo_detect.py
    python yolo_detect.py --model yolov8s.pt --confidence 0.5
    python yolo_detect.py --show

Fields produced (per tracked class, up to 3 instances each):
    person_count            — number of people detected (normalized 0-1)
    person_0_detected       — instance presence flag
    person_0_cx/cy          — bounding box center (normalized)
    person_0_w/h            — bounding box size (normalized)
    person_0_conf           — detection confidence
    ...
    total_objects           — total detection count (normalized)
"""

import time
import sys

try:
    import cv2
except ImportError:
    print("Install: pip install opencv-python")
    sys.exit(1)

try:
    from ultralytics import YOLO
except ImportError:
    print("Install: pip install ultralytics")
    sys.exit(1)

from phosphor_bridge import PhosphorBridge

# Classes we expose as bindable sources (most useful for performance)
TRACKED_CLASSES = {
    0: "person",
    67: "cell_phone",
    73: "book",
    39: "bottle",
}
MAX_INSTANCES = 3  # track up to N instances per class


def build_schema():
    fields = {}

    # Per-class counts
    for cls_id, cls_name in TRACKED_CLASSES.items():
        fields[f"{cls_name}_count"] = {
            "min": 0, "max": 10,
            "label": f"{cls_name.title()} Count",
        }

        # Per-instance bounding box (up to MAX_INSTANCES)
        for i in range(MAX_INSTANCES):
            prefix = f"{cls_name}_{i}"
            fields[f"{prefix}_detected"] = {
                "min": 0, "max": 1,
                "label": f"{cls_name.title()} {i} Detected",
                "is_trigger": True,
            }
            fields[f"{prefix}_cx"] = {
                "min": 0, "max": 1,
                "label": f"{cls_name.title()} {i} Center X",
            }
            fields[f"{prefix}_cy"] = {
                "min": 0, "max": 1,
                "label": f"{cls_name.title()} {i} Center Y",
            }
            fields[f"{prefix}_w"] = {
                "min": 0, "max": 1,
                "label": f"{cls_name.title()} {i} Width",
            }
            fields[f"{prefix}_h"] = {
                "min": 0, "max": 1,
                "label": f"{cls_name.title()} {i} Height",
            }
            fields[f"{prefix}_conf"] = {
                "min": 0, "max": 1,
                "label": f"{cls_name.title()} {i} Confidence",
            }

    # Global
    fields["total_objects"] = {
        "min": 0, "max": 30,
        "label": "Total Objects",
    }

    return fields


def main():
    parser = PhosphorBridge.common_args("Phosphor Bridge — YOLO Detection")
    parser.add_argument("--device", type=int, default=0,
                        help="Camera device index")
    parser.add_argument("--model", default="yolov8n.pt",
                        help="YOLO model (yolov8n/s/m/l/x.pt)")
    parser.add_argument("--confidence", type=float, default=0.4,
                        help="Detection confidence threshold")
    parser.add_argument("--show", action="store_true",
                        help="Show detection preview")
    args = parser.parse_args()

    # Init YOLO
    model = YOLO(args.model)
    print(f"[yolo-detect] Model: {args.model}")

    # Init camera
    cap = cv2.VideoCapture(args.device)
    w = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH))
    h = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
    print(f"[yolo-detect] Camera {args.device}: {w}x{h}")

    # Init bridge
    bridge = PhosphorBridge("yolo-detect", args.host, args.port)
    schema = build_schema()
    bridge.declare_fields(schema)
    print(f"[yolo-detect] Tracking: {list(TRACKED_CLASSES.values())}")
    print(f"[yolo-detect] Fields: {len(schema)}")

    if not bridge.connect():
        return

    dt = 1.0 / args.fps
    print(f"[yolo-detect] Streaming at {args.fps} Hz. Ctrl+C to stop.")

    try:
        while True:
            t0 = time.time()

            ret, frame = cap.read()
            if not ret:
                time.sleep(0.01)
                continue

            # Run inference
            results = model(frame, conf=args.confidence, verbose=False)[0]
            boxes = results.boxes

            data = {}
            total = 0

            # Group detections by class
            class_instances = {cid: [] for cid in TRACKED_CLASSES}

            for box in boxes:
                cls_id = int(box.cls[0])
                if cls_id in TRACKED_CLASSES:
                    # Normalized bounding box
                    x1, y1, x2, y2 = box.xyxyn[0].tolist()
                    cx = (x1 + x2) / 2
                    cy = (y1 + y2) / 2
                    bw = x2 - x1
                    bh = y2 - y1
                    conf = float(box.conf[0])

                    class_instances[cls_id].append({
                        "cx": cx, "cy": cy, "w": bw, "h": bh,
                        "conf": conf,
                    })
                    total += 1

            # Write to fields
            for cls_id, cls_name in TRACKED_CLASSES.items():
                instances = class_instances[cls_id]
                # Sort by confidence descending
                instances.sort(key=lambda x: x["conf"], reverse=True)

                data[f"{cls_name}_count"] = min(10, len(instances)) / 10.0

                for i in range(MAX_INSTANCES):
                    prefix = f"{cls_name}_{i}"
                    if i < len(instances):
                        inst = instances[i]
                        data[f"{prefix}_detected"] = 1.0
                        data[f"{prefix}_cx"] = inst["cx"]
                        data[f"{prefix}_cy"] = inst["cy"]
                        data[f"{prefix}_w"] = inst["w"]
                        data[f"{prefix}_h"] = inst["h"]
                        data[f"{prefix}_conf"] = inst["conf"]
                    else:
                        data[f"{prefix}_detected"] = 0.0
                        data[f"{prefix}_cx"] = 0.0
                        data[f"{prefix}_cy"] = 0.0
                        data[f"{prefix}_w"] = 0.0
                        data[f"{prefix}_h"] = 0.0
                        data[f"{prefix}_conf"] = 0.0

            data["total_objects"] = min(1.0, total / 30.0)

            bridge.push(data)

            if args.show:
                annotated = results.plot()
                cv2.imshow("Phosphor — YOLO Detect", annotated)
                if cv2.waitKey(1) & 0xFF == 27:
                    break

            elapsed = time.time() - t0
            if elapsed < dt:
                time.sleep(dt - elapsed)

    finally:
        cap.release()
        if args.show:
            cv2.destroyAllWindows()
        bridge.shutdown()


if __name__ == "__main__":
    main()
