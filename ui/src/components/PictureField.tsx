// The setting that holds a small picture for a matrix. Whatever image is
// chosen is shrunk here, in the browser, to about as many dots as the
// largest matrix has, and sent to the engine as plain pixels: the engine
// never opens an image file.

import { ImageUp } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { engine, useShow, type ParamSpec } from "../engine";

/** Reads "16x16:base64" into its size and pixels, or null. */
function read(value: string): { width: number; height: number; rgb: Uint8Array } | null {
  const match = /^(\d+)x(\d+):(.*)$/.exec(value);
  if (!match) return null;
  const [width, height] = [Number(match[1]), Number(match[2])];
  try {
    const rgb = Uint8Array.from(atob(match[3]!), (c) => c.charCodeAt(0));
    return rgb.length === width * height * 3 ? { width, height, rgb } : null;
  } catch {
    return null;
  }
}

/** Shrinks an image file to at most `side` dots along its longer side. */
async function shrink(file: File, side: number): Promise<string> {
  const bitmap = await createImageBitmap(file);
  const scale = Math.min(1, side / Math.max(bitmap.width, bitmap.height));
  const width = Math.max(1, Math.round(bitmap.width * scale));
  const height = Math.max(1, Math.round(bitmap.height * scale));
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("no canvas");
  // See-through parts of the image are dark on the lights.
  ctx.fillStyle = "#000";
  ctx.fillRect(0, 0, width, height);
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(bitmap, 0, 0, width, height);
  bitmap.close();
  const rgba = ctx.getImageData(0, 0, width, height).data;
  let binary = "";
  for (let i = 0; i < rgba.length; i += 4) binary += String.fromCharCode(rgba[i]!, rgba[i + 1]!, rgba[i + 2]!);
  return `${width}x${height}:${btoa(binary)}`;
}

export function PictureField({ spec, value, layer }: { spec: ParamSpec; value: string; layer: number }) {
  const show = useShow();
  const preview = useRef<HTMLCanvasElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<string | null>(null);
  const picture = read(value);

  useEffect(() => {
    const canvas = preview.current;
    if (!canvas || !picture) return;
    canvas.width = picture.width;
    canvas.height = picture.height;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const image = ctx.createImageData(picture.width, picture.height);
    for (let i = 0, o = 0; i < picture.rgb.length; i += 3, o += 4) {
      image.data[o] = picture.rgb[i]!;
      image.data[o + 1] = picture.rgb[i + 1]!;
      image.data[o + 2] = picture.rgb[i + 2]!;
      image.data[o + 3] = 255;
    }
    ctx.putImageData(image, 0, 0);
    // The text itself says when the picture changed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value]);

  // As many dots as the largest matrix has along its longer side; more
  // would be thrown away by the lights anyway.
  const largest = Math.max(
    0,
    ...(show?.fixtures ?? []).map((f) => (f.shape.type === "matrix" ? Math.max(f.shape.cols, f.shape.rows) : 0)),
  );
  const side = Math.min(spec.max ?? 64, largest || 32);

  const choose = async (file: File | undefined) => {
    if (!file) return;
    try {
      const text = await shrink(file, side);
      setError(null);
      engine.send({ type: "set_param", layer, param: spec.id, value: text });
    } catch {
      setError("That file could not be read as a picture.");
    }
  };

  return (
    <div className="field">
      <span className="field-head">
        <span>{spec.label}</span>
        <span className="value">{picture ? `${picture.width} × ${picture.height} dots` : "None"}</span>
      </span>
      <div className="picture-field">
        <canvas ref={preview} className="picture-preview" aria-label="The picture as the lights get it" />
        <button type="button" className="button small" onClick={() => input.current?.click()}>
          <ImageUp size={14} /> Choose a picture…
        </button>
        <input
          ref={input}
          type="file"
          accept="image/*"
          hidden
          aria-label="Picture file"
          onChange={(e) => {
            void choose(e.target.files?.[0]);
            // So the same file can be chosen again after a change.
            e.target.value = "";
          }}
        />
      </div>
      <p className="hint">
        Any image works. It is shrunk to at most {side} dots a side to suit your lights, so simple pictures with
        strong colours look best.
      </p>
      {error && <p className="callout error">{error}</p>}
    </div>
  );
}
