// Draws the LEDs: one soft glowing dot per pixel, blended additively so
// neighbouring lights merge the way real LEDs do behind a diffuser.

import { engine, type Fixture } from "./engine";
import { runLength } from "./components/ShapeEditor";

export interface View {
  /** Canvas position shown at the centre of the stage. */
  cx: number;
  cy: number;
  /** Screen pixels per canvas unit. */
  scale: number;
}

const VERTEX = `#version 300 es
in vec2 a_pos;
in vec3 a_color;
in float a_size;
uniform vec2 u_center;
uniform vec2 u_scale;     // canvas units to clip space
uniform float u_px;       // canvas units to device pixels
out vec3 v_color;
void main() {
  gl_Position = vec4((a_pos - u_center) * u_scale, 0.0, 1.0);
  // The quad is wider than the LED itself to leave room for the glow.
  gl_PointSize = clamp(a_size * u_px * 2.6, 3.0, 256.0);
  v_color = a_color;
}`;

const FRAGMENT = `#version 300 es
precision mediump float;
in vec3 v_color;
out vec4 o;
void main() {
  float d = length(gl_PointCoord - 0.5) * 2.0;
  // The lit disc is about two thirds of the gap between LEDs, so
  // neighbours stay distinct; the glow reaches a little past them.
  float core = smoothstep(0.29, 0.22, d);
  float glow = exp(-d * d * 7.0) * 0.30;
  float fade = smoothstep(1.0, 0.8, d);
  // A dim grey disc marks an LED that is off, so the layout stays visible.
  vec3 body = vec3(0.055) * core;
  o = vec4(body + v_color * (core + glow) * fade, 1.0);
}`;

function compile(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader {
  const shader = gl.createShader(type)!;
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(shader) ?? "shader failed to compile");
  }
  return shader;
}

/** Spacing between neighbouring LEDs of each fixture, per pixel. */
function pixelSizes(fixtures: Fixture[], total: number): Float32Array {
  const sizes = new Float32Array(total);
  let offset = 0;
  for (const f of fixtures) {
    const count = f.shape.type === "strip" ? f.shape.count : f.shape.cols * f.shape.rows;
    const spacing =
      f.shape.type === "strip"
        ? runLength(f) / Math.max(1, f.shape.count)
        : Math.min(f.width / Math.max(1, f.shape.cols), f.height / Math.max(1, f.shape.rows));
    sizes.fill(spacing, offset, Math.min(total, offset + count));
    offset += count;
  }
  return sizes;
}

export class StageRenderer {
  private gl: WebGL2RenderingContext;
  private program: WebGLProgram;
  private vao: WebGLVertexArrayObject;
  private posBuffer: WebGLBuffer;
  private colorBuffer: WebGLBuffer;
  private sizeBuffer: WebGLBuffer;
  private uCenter: WebGLUniformLocation;
  private uScale: WebGLUniformLocation;
  private uPx: WebGLUniformLocation;
  private count = 0;
  private builtFor = -1;
  private sizesFor: Fixture[] | null = null;
  private black = new Uint8Array(0);

  constructor(canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl2", { antialias: false, alpha: false, powerPreference: "low-power" });
    if (!gl) throw new Error("This browser cannot show the preview (WebGL 2 is not available).");
    this.gl = gl;
    const program = gl.createProgram()!;
    gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, VERTEX));
    gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, FRAGMENT));
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      throw new Error(gl.getProgramInfoLog(program) ?? "preview shader failed to link");
    }
    this.program = program;
    this.uCenter = gl.getUniformLocation(program, "u_center")!;
    this.uScale = gl.getUniformLocation(program, "u_scale")!;
    this.uPx = gl.getUniformLocation(program, "u_px")!;

    this.vao = gl.createVertexArray()!;
    gl.bindVertexArray(this.vao);
    this.posBuffer = this.attribute("a_pos", 2, gl.FLOAT, false);
    this.colorBuffer = this.attribute("a_color", 3, gl.UNSIGNED_BYTE, true);
    this.sizeBuffer = this.attribute("a_size", 1, gl.FLOAT, false);
    gl.bindVertexArray(null);
  }

  private attribute(name: string, size: number, type: number, normalized: boolean): WebGLBuffer {
    const gl = this.gl;
    const buffer = gl.createBuffer()!;
    const location = gl.getAttribLocation(this.program, name);
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.enableVertexAttribArray(location);
    gl.vertexAttribPointer(location, size, type, normalized, 0, 0);
    return buffer;
  }

  draw(view: View, fixtures: Fixture[], dpr: number): void {
    const gl = this.gl;
    const canvas = gl.canvas as HTMLCanvasElement;
    gl.viewport(0, 0, canvas.width, canvas.height);
    gl.clearColor(0.027, 0.03, 0.04, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);

    const count = engine.positions.length / 2;
    if (this.builtFor !== engine.layoutVersion) {
      this.builtFor = engine.layoutVersion;
      this.count = count;
      this.sizesFor = null;
      this.black = new Uint8Array(count * 3);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.posBuffer);
      gl.bufferData(gl.ARRAY_BUFFER, engine.positions, gl.STATIC_DRAW);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.colorBuffer);
      gl.bufferData(gl.ARRAY_BUFFER, this.black, gl.DYNAMIC_DRAW);
    }
    if (this.sizesFor !== fixtures) {
      this.sizesFor = fixtures;
      gl.bindBuffer(gl.ARRAY_BUFFER, this.sizeBuffer);
      gl.bufferData(gl.ARRAY_BUFFER, pixelSizes(fixtures, this.count), gl.STATIC_DRAW);
    }
    if (this.count === 0) return;

    // Colours belong to lights in order, wherever those lights are. So a
    // frame rendered just before a light was moved is still right for it,
    // and is kept until the next one arrives: blanking in between made
    // lights flicker while they were dragged. Only a change in how many
    // lights there are leaves nothing to show.
    const fresh = engine.colors.length === this.count * 3;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.colorBuffer);
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, fresh ? engine.colors : this.black);

    gl.useProgram(this.program);
    gl.uniform2f(this.uCenter, view.cx, view.cy);
    gl.uniform2f(this.uScale, (2 * view.scale * dpr) / canvas.width, (-2 * view.scale * dpr) / canvas.height);
    gl.uniform1f(this.uPx, view.scale * dpr);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE);
    gl.bindVertexArray(this.vao);
    gl.drawArrays(gl.POINTS, 0, this.count);
    gl.bindVertexArray(null);
  }
}
