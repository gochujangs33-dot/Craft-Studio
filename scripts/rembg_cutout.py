"""
ArtCraft Studio - AI Background Removal (rembg) Engine
High-fidelity subject / portrait segmentation using local ONNX / rembg
"""
import sys
import os

def main():
    if len(sys.argv) < 3:
        print("Usage: python rembg_cutout.py <input_image> <output_image>", file=sys.stderr)
        sys.exit(1)

    in_path = sys.argv[1]
    out_path = sys.argv[2]

    if not os.path.isfile(in_path):
        print(f"Error: input file '{in_path}' does not exist.", file=sys.stderr)
        sys.exit(2)

    try:
        from rembg import remove
        from PIL import Image

        img = Image.open(in_path)
        # Ensure RGBA
        out = remove(img)
        # Make parent dirs
        os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
        out.save(out_path, format="PNG")
        print("SUCCESS")
        sys.exit(0)
    except Exception as e:
        print(f"Error during rembg execution: {e}", file=sys.stderr)
        sys.exit(3)

if __name__ == "__main__":
    main()
