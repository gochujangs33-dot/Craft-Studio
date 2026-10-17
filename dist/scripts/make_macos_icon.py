#!/usr/bin/env python3
"""
Generates modern macOS Big Sur / Sonoma style AppIcon.icns and AppIcon.png
for ArtCraft Studio.
"""

import math
import os
from PIL import Image, ImageDraw, ImageFilter, ImageOps, ImageChops

def create_squircle_mask(size, radius):
    """Generates an antialiased squircle / continuous rounded rectangle mask."""
    scale = 4
    w, h = size[0] * scale, size[1] * scale
    r = radius * scale
    mask = Image.new('L', (w, h), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle([0, 0, w - 1, h - 1], radius=r, fill=255)
    return mask.resize(size, Image.Resampling.LANCZOS)

def create_gradient(width, height, color_top, color_bottom):
    """Creates a vertical linear gradient image."""
    base = Image.new('RGBA', (width, height), color_top)
    gradient = Image.new('L', (1, height))
    for y in range(height):
        gradient.putpixel((0, y), int(255 * (y / float(height - 1))))
    gradient = gradient.resize((width, height))
    bottom = Image.new('RGBA', (width, height), color_bottom)
    base.paste(bottom, (0, 0), gradient)
    return base

def main():
    root_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    assets_dir = os.path.join(root_dir, 'launcher', 'assets')
    os.makedirs(assets_dir, exist_ok=True)
    
    canvas_size = 1024
    squircle_size = 824
    radius = 185
    margin = (canvas_size - squircle_size) // 2
    
    # 1. Canvas
    canvas = Image.new('RGBA', (canvas_size, canvas_size), (0, 0, 0, 0))
    
    # 2. Drop Shadow for Squircle
    shadow_offset_y = 16
    shadow_mask = create_squircle_mask((squircle_size, squircle_size), radius)
    shadow_layer = Image.new('RGBA', (canvas_size, canvas_size), (0, 0, 0, 0))
    shadow_color = Image.new('RGBA', (squircle_size, squircle_size), (0, 0, 0, 110))
    shadow_layer.paste(shadow_color, (margin, margin + shadow_offset_y), shadow_mask)
    # Blur shadow
    shadow_blurred = shadow_layer.filter(ImageFilter.GaussianBlur(radius=28))
    canvas = Image.alpha_composite(canvas, shadow_blurred)
    
    # 3. Squircle Base
    squircle_mask = create_squircle_mask((squircle_size, squircle_size), radius)
    # Gradient background: deep rich dark indigo to midnight obsidian
    bg_gradient = create_gradient(squircle_size, squircle_size, (26, 32, 54, 255), (10, 12, 22, 255))
    
    # Subtle inner top specular highlight
    sheen = create_gradient(squircle_size, squircle_size // 2, (255, 255, 255, 30), (255, 255, 255, 0))
    bg_gradient.paste(sheen, (0, 0), sheen)
    
    # Subtle inner border stroke
    stroke_mask_outer = squircle_mask
    stroke_mask_inner = create_squircle_mask((squircle_size - 4, squircle_size - 4), radius - 2)
    inner_placed = Image.new('L', (squircle_size, squircle_size), 0)
    inner_placed.paste(stroke_mask_inner, (2, 2))
    stroke_mask = ImageChops.difference(stroke_mask_outer, inner_placed)
    
    stroke_gradient = create_gradient(squircle_size, squircle_size, (90, 105, 160, 200), (35, 42, 68, 120))
    bg_gradient.paste(stroke_gradient, (0, 0), stroke_mask)
    
    # Composite squircle onto canvas
    canvas.paste(bg_gradient, (margin, margin), squircle_mask)
    
    # 4. Prepare Center Logo Emblem
    logo_path = os.path.join(assets_dir, 'app-256.png')
    if os.path.exists(logo_path):
        logo = Image.open(logo_path).convert('RGBA')
        logo_size = 520
        logo_scaled = logo.resize((logo_size, logo_size), Image.Resampling.LANCZOS)
        
        # Center coordinates
        logo_x = (canvas_size - logo_size) // 2
        logo_y = (canvas_size - logo_size) // 2 - 6 # slight optical center bias
        
        # Subtle logo drop shadow
        logo_alpha = logo_scaled.split()[3]
        logo_shadow = Image.new('RGBA', (canvas_size, canvas_size), (0, 0, 0, 0))
        logo_shadow_color = Image.new('RGBA', (logo_size, logo_size), (0, 4, 16, 160))
        logo_shadow.paste(logo_shadow_color, (logo_x, logo_y + 12), logo_alpha)
        logo_shadow = logo_shadow.filter(ImageFilter.GaussianBlur(radius=16))
        
        # Clip logo shadow inside squircle
        squircle_full_mask = Image.new('L', (canvas_size, canvas_size), 0)
        squircle_full_mask.paste(squircle_mask, (margin, margin))
        
        canvas = Image.alpha_composite(canvas, Image.composite(logo_shadow, Image.new('RGBA', (canvas_size, canvas_size), (0, 0, 0, 0)), squircle_full_mask))
        
        # Colorize logo with vibrant gradient
        logo_grad = create_gradient(logo_size, logo_size, (92, 160, 255, 255), (55, 110, 245, 255))
        logo_colored = Image.new('RGBA', (logo_size, logo_size), (0, 0, 0, 0))
        logo_colored.paste(logo_grad, (0, 0), logo_alpha)
        
        # Top specular edge on logo
        edge_filter = logo_alpha.filter(ImageFilter.FIND_EDGES)
        edge_glow = Image.new('RGBA', (logo_size, logo_size), (200, 230, 255, 70))
        logo_colored.paste(edge_glow, (0, -1), edge_filter)
        
        canvas.paste(logo_colored, (logo_x, logo_y), logo_alpha)
        
    out_png = os.path.join(assets_dir, 'AppIcon.png')
    out_icns = os.path.join(assets_dir, 'AppIcon.icns')
    
    # Save 1024x1024 master PNG
    canvas.save(out_png, 'PNG')
    print(f"Saved: {out_png}")
    
    # Save multi-size ICNS
    sizes = [16, 32, 64, 128, 256, 512, 1024]
    icon_images = []
    for s in sizes:
        resized = canvas.resize((s, s), Image.Resampling.LANCZOS)
        icon_images.append(resized)
    
    canvas.save(out_icns, format='ICNS', append_images=icon_images)
    print(f"Saved: {out_icns} (sizes: {sizes})")

if __name__ == '__main__':
    main()
