# Custom Data Integration & D3.js Charting Spec

A developer spec for two things:

1. Plugging your own data source into the OpenBB Platform.
2. Replacing the existing Plotly `surface3d` renderer with a D3.js + WebGL implementation.

Target audience: developers extending OpenBB Platform >= the version in this repo.

---

## Part 1 — Plugging In Your Own Data

OpenBB exposes data through **provider extensions**. A provider is a Python package that ships three pieces per endpoint (`QueryParams`, `Data`, `Fetcher`), declares itself via an entry point in `pyproject.toml`, and is then auto-discovered by the core router.

### 1.1 Architecture at a glance

```
your_extension/
├── pyproject.toml                          # entry-point registration
└── openbb_<name>/
    ├── __init__.py                         # builds the Provider() object
    ├── models/
    │   └── <endpoint>.py                   # QueryParams + Data + Fetcher
    └── utils/
        └── helpers.py                      # HTTP / parsing helpers
```

Reference implementations:
- `openbb_platform/providers/fmp/openbb_fmp/`
- `openbb_platform/providers/polygon/openbb_polygon/`
- Scaffold a fresh one: `cookiecutter/{{cookiecutter.project_tag}}/`

### 1.2 The three classes (contract)

Per endpoint, in `openbb_<name>/models/<endpoint>.py`:

| Class           | Inherits from                                 | Purpose                                                                 |
| --------------- | --------------------------------------------- | ----------------------------------------------------------------------- |
| `QueryParams`   | `EquityHistoricalQueryParams` (or other std.) | Validate request inputs; add provider-specific knobs (`interval`, etc.) |
| `Data`          | `EquityHistoricalData` (or other std.)        | Output schema. Use `__alias_dict__` to map vendor JSON keys → std names |
| `Fetcher`       | `Fetcher[QueryParams, list[Data]]`            | `transform_query` → `aextract_data` (async HTTP) → `transform_data`     |

Skeleton:

```python
from openbb_core.provider.abstract.fetcher import Fetcher
from openbb_core.provider.standard_models.equity_historical import (
    EquityHistoricalData,
    EquityHistoricalQueryParams,
)
from pydantic import Field

class MyQueryParams(EquityHistoricalQueryParams):
    interval: str = Field(default="1d")

class MyData(EquityHistoricalData):
    __alias_dict__ = {"open": "o", "close": "c", "high": "h", "low": "l", "date": "t"}

class MyFetcher(Fetcher[MyQueryParams, list[MyData]]):
    @staticmethod
    def transform_query(params: dict) -> MyQueryParams:
        return MyQueryParams(**params)

    @staticmethod
    async def aextract_data(query, credentials, **_) -> list[dict]:
        # call your API, return list[dict]
        ...

    @staticmethod
    def transform_data(query, data, **_) -> list[MyData]:
        return [MyData.model_validate(d) for d in data]
```

### 1.3 Standard models (why inherit?)

Standard models live in `openbb_platform/core/openbb_core/provider/standard_models/`. They define the cross-provider contract — every provider answering `obb.equity.price.historical(...)` returns the same canonical fields (`date`, `open`, `high`, `low`, `close`, `volume`, `vwap`). Your `Data` class is free to add fields on top; clients that only know the standard fields keep working.

Pick the closest standard model to your endpoint before inventing one. If nothing fits, talk to the maintainers — adding a new standard model is a core-package change, not a provider change.

### 1.4 Provider registration

`openbb_<name>/__init__.py`:

```python
from openbb_core.provider.abstract.provider import Provider
from openbb_<name>.models.<endpoint> import MyFetcher

<name>_provider = Provider(
    name="<name>",
    website="https://example.com",
    credentials=["api_key"],
    fetcher_dict={"EquityHistorical": MyFetcher},
)
```

`pyproject.toml`:

```toml
[tool.poetry.plugins."openbb_provider_extension"]
<name> = "openbb_<name>:<name>_provider"
```

### 1.5 Installation & use

```bash
cd your_extension && pip install -e .
python -c "from openbb import obb; print(obb.equity.price.historical('AAPL', provider='<name>'))"
```

API keys are read from `~/.openbb_platform/user_settings.json` under `credentials.<name>_api_key`, or the matching env var.

### 1.6 Custom data with no API (CSV, parquet, in-memory)

Same contract — just skip the HTTP step:

```python
@staticmethod
async def aextract_data(query, credentials, **_):
    import pandas as pd
    return pd.read_parquet(f"/data/{query.symbol}.parquet").to_dict("records")
```

### 1.7 Scaffolding shortcut

```bash
pip install cookiecutter
cookiecutter openbb_platform/cookiecutter
# answer the prompts; pick extension_type = provider
pip install -e ./<your_project>
```

The generated `models/ohlc_example.py` is a working template — copy it per endpoint.

### 1.8 Checklist

- [ ] `pyproject.toml` declares `openbb_provider_extension` entry point.
- [ ] Each endpoint has `QueryParams` + `Data` + `Fetcher`.
- [ ] `Data` inherits the matching standard model (or justify a new one).
- [ ] `__alias_dict__` maps vendor JSON keys → std names — no field renaming in Python.
- [ ] `aextract_data` is `async` and idempotent.
- [ ] `transform_data` always returns `list[Data]`, never raw dicts.
- [ ] Unit test under `tests/` using `pytest` + recorded responses (`pytest-vcr`).

---

## Part 2 — D3.js Replacement for `surface3d`

The current implementation lives at `openbb_platform/obbject_extensions/charting/openbb_charting/charts/generic_charts.py:633-789`. It builds a Plotly `mesh3d` from a (X, Y, Z) point cloud via 2D Delaunay triangulation. The replacement keeps the **Python-side contract identical** so callers don't change, but emits a JSON spec consumed by a D3.js renderer in the browser.

### 2.1 Why D3 (with WebGL via three.js)

- D3 alone has no 3D primitive — but `d3-delaunay` gives us the same triangulation Plotly uses, and `three.js` (WebGL) handles the actual mesh rendering and lighting.
- The split: **D3 owns scales, color interpolation, triangulation, and DOM/SVG axes**; **three.js owns the mesh, camera, and lighting**.
- Result: smaller payload than Plotly (~30 KB vs ~3 MB), full styling control, no Plotly licensing footprint in the web bundle.

### 2.2 Data contract (unchanged from Plotly version)

```
Input:  X: pd.Series, Y: pd.Series, Z: pd.Series   # equal length N, numeric
Output: dict (JSON-serializable spec) — see §2.4
```

The Python signature stays:

```python
def surface3d(
    X, Y, Z,
    xtitle="DTE", ytitle="Strike", ztitle="IV",
    colorscale=None,
    title=None,
    layout_kwargs=None,
    theme="dark",
) -> dict:                         # was OpenBBFigure
    ...
```

`obbject.charting.show()` is responsible for handing the spec to the browser front-end (Electron `desktop/`, or a notebook display hook).

### 2.3 Python side — replacement function

Drop-in replacement for the body of `surface3d`. Triangulation and color mapping stay in Python (numpy / scipy already a dependency), so the front-end gets ready-to-render buffers.

```python
def surface3d(X, Y, Z, xtitle="DTE", ytitle="Strike", ztitle="IV",
              colorscale=None, title=None, layout_kwargs=None, theme="dark"):
    import numpy as np
    from scipy.spatial import Delaunay
    from openbb_core.app.model.abstract.error import OpenBBError

    X = np.asarray(X, dtype=float)
    Y = np.asarray(Y, dtype=float)
    Z = np.asarray(Z, dtype=float)
    if not (len(X) == len(Y) == len(Z) >= 3):
        raise OpenBBError("surface3d requires >=3 points of equal length")

    try:
        tri = Delaunay(np.column_stack([X, Y]))
    except Exception as e:
        raise OpenBBError(f"Delaunay failed: {e}") from e

    return {
        "type": "surface3d",
        "renderer": "d3-three",            # front-end picks this up
        "title": title or "",
        "axes": {"x": xtitle, "y": ytitle, "z": ztitle},
        "theme": theme,
        "vertices": np.column_stack([X, Y, Z]).ravel().tolist(),  # flat xyz
        "indices":  tri.simplices.ravel().tolist(),               # flat ijk
        "intensity": Z.tolist(),
        "domain": {
            "x": [float(X.min()), float(X.max())],
            "y": [float(Y.min()), float(Y.max())],
            "z": [float(Z.min()), float(Z.max())],
        },
        "colorscale": colorscale or _DEFAULT_VOL_COLORSCALE,
        "camera":   {"eye": [1.75, 1.75, 0.69], "up": [0, 0, 1], "center": [-0.01, 0, -0.3]},
        "aspect":   [1.5, 2.0, 0.75],
        "lighting": {"ambient": 0.95, "diffuse": 0.9, "specular": 0.9, "roughness": 0.8},
        "layout_kwargs": layout_kwargs or {},
    }
```

`_DEFAULT_VOL_COLORSCALE` is the existing 14-stop red→blue list, lifted verbatim from the current code.

### 2.4 JSON spec (browser contract)

| Field          | Type         | Notes                                                              |
| -------------- | ------------ | ------------------------------------------------------------------ |
| `vertices`     | `number[]`   | Flat `[x0,y0,z0, x1,y1,z1, ...]`, length `3N`                      |
| `indices`      | `number[]`   | Flat `[i0,j0,k0, ...]`, length `3T` for T triangles                |
| `intensity`    | `number[]`   | Length `N`, drives color per vertex                                |
| `colorscale`   | `[t, css][]` | Stops in `[0,1]`; D3 uses `d3.scaleLinear().interpolate(rgb)`      |
| `camera`       | `{eye,up,center}` of `[x,y,z]` | Mapped onto `THREE.PerspectiveCamera`            |
| `aspect`       | `[x,y,z]`    | Scene scaling                                                      |
| `theme`        | `"dark"`/`"light"` | Background, axis label colors                                |

### 2.5 Front-end (TypeScript)

`desktop/src/charts/surface3d.ts`:

```ts
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { scaleLinear } from "d3-scale";
import { rgb } from "d3-color";
import { interpolateRgb } from "d3-interpolate";

export interface Surface3DSpec {
  vertices: number[];
  indices: number[];
  intensity: number[];
  colorscale: [number, string][];
  camera: { eye: [number, number, number]; up: [number, number, number]; center: [number, number, number] };
  aspect: [number, number, number];
  axes: { x: string; y: string; z: string };
  theme: "dark" | "light";
}

export function renderSurface3D(container: HTMLElement, spec: Surface3DSpec) {
  const w = container.clientWidth, h = container.clientHeight;
  const scene = new THREE.Scene();
  scene.background = new THREE.Color(spec.theme === "dark" ? 0x1e1e1e : 0xffffff);

  const camera = new THREE.PerspectiveCamera(45, w / h, 0.1, 1000);
  camera.position.set(...spec.camera.eye).multiplyScalar(3);
  camera.up.set(...spec.camera.up);

  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setSize(w, h);
  container.appendChild(renderer.domElement);

  // --- color per vertex via D3 ---
  const stops = spec.colorscale;
  const domain = stops.map(([t]) => t);
  const range = stops.map(([, c]) => c);
  const color = scaleLinear<string>().domain(domain).range(range).interpolate(interpolateRgb);

  const zMin = Math.min(...spec.intensity), zMax = Math.max(...spec.intensity);
  const colors = new Float32Array(spec.intensity.length * 3);
  spec.intensity.forEach((z, i) => {
    const c = rgb(color((z - zMin) / (zMax - zMin)));
    colors[i * 3] = c.r / 255; colors[i * 3 + 1] = c.g / 255; colors[i * 3 + 2] = c.b / 255;
  });

  // --- geometry from flat buffers ---
  const geom = new THREE.BufferGeometry();
  geom.setAttribute("position", new THREE.Float32BufferAttribute(spec.vertices, 3));
  geom.setAttribute("color",    new THREE.Float32BufferAttribute(colors, 3));
  geom.setIndex(spec.indices);
  geom.computeVertexNormals();

  const mat = new THREE.MeshPhongMaterial({
    vertexColors: true, flatShading: true, side: THREE.DoubleSide,
    shininess: 30,
  });
  const mesh = new THREE.Mesh(geom, mat);
  mesh.scale.set(...spec.aspect);
  scene.add(mesh);

  scene.add(new THREE.AmbientLight(0xffffff, 0.95));
  const dir = new THREE.DirectionalLight(0xffffff, 0.9);
  dir.position.set(1, 1, 1);
  scene.add(dir);

  // --- D3 owns the SVG axes overlay ---
  drawAxes(container, spec);            // see §2.6

  const controls = new OrbitControls(camera, renderer.domElement);
  controls.target.set(...spec.camera.center);
  (function loop() {
    requestAnimationFrame(loop);
    controls.update();
    renderer.render(scene, camera);
  })();
}
```

Required deps in `desktop/package.json`:

```json
"three": "^0.160",
"d3-scale": "^4",
"d3-color": "^3",
"d3-interpolate": "^3",
"d3-delaunay": "^6"     // only if triangulating client-side
```

### 2.6 Axes & legend (the D3 part)

Two SVG overlays positioned absolutely over the WebGL canvas:

- **Axis labels** (`spec.axes.{x,y,z}`): static SVG text, anchored to projected corner positions.
- **Color legend**: `d3.scaleLinear` mapped to a vertical gradient rect; ticks from `d3-axis`.

Skeleton:

```ts
function drawAxes(container: HTMLElement, spec: Surface3DSpec) {
  const svg = d3.select(container).append("svg")
    .attr("class", "surface3d-overlay")
    .style("position", "absolute").style("inset", "0").style("pointer-events", "none");

  // color legend
  const legend = svg.append("g").attr("transform", "translate(20,40)");
  const gradId = "surface3d-grad";
  const defs = svg.append("defs").append("linearGradient")
    .attr("id", gradId).attr("x1", "0").attr("y1", "1").attr("x2", "0").attr("y2", "0");
  spec.colorscale.forEach(([t, c]) =>
    defs.append("stop").attr("offset", `${t * 100}%`).attr("stop-color", c));
  legend.append("rect").attr("width", 12).attr("height", 200).attr("fill", `url(#${gradId})`);

  const scale = d3.scaleLinear()
    .domain([Math.min(...spec.intensity), Math.max(...spec.intensity)])
    .range([200, 0]);
  legend.append("g").attr("transform", "translate(12,0)").call(d3.axisRight(scale).ticks(6));
}
```

### 2.7 Behavioral parity matrix

| Feature                       | Plotly today                 | D3+three.js replacement                                              |
| ----------------------------- | ---------------------------- | -------------------------------------------------------------------- |
| Mesh from scattered XYZ       | `fig.add_mesh3d`             | `THREE.BufferGeometry` + `Mesh`                                      |
| Delaunay triangulation        | `scipy.spatial.Delaunay`     | Same (Python side) — or `d3-delaunay` if client-side                 |
| Per-vertex color by intensity | `intensity=Z`, `colorscale=` | `d3-scale` + `d3-interpolate` → `BufferAttribute("color")`           |
| Lighting                      | Plotly `lighting=`           | `MeshPhongMaterial` + `AmbientLight` + `DirectionalLight`            |
| Camera                        | `scene_camera`               | `PerspectiveCamera.position` + `up`                                  |
| Drag / orbit                  | `dragmode="turntable"`       | `OrbitControls`                                                      |
| Hover readout                 | `hovertemplate`              | Raycaster → tooltip div (see §2.8)                                   |
| Axis titles, gridlines        | `scene.{x,y,z}axis`          | SVG overlay (D3) + `THREE.GridHelper`                                |
| Contour lines on surface      | `contour=dict(...)`          | Compute isolines from `intensity` in a fragment shader, or skip v1   |
| Aspect ratio                  | `aspectratio`                | `mesh.scale.set(...)`                                                |
| Theme                         | `ChartStyle().plt_style`     | Background color + axis font color toggled by `spec.theme`           |

### 2.8 Hover

```ts
const raycaster = new THREE.Raycaster();
renderer.domElement.addEventListener("mousemove", (e) => {
  const rect = renderer.domElement.getBoundingClientRect();
  const mouse = new THREE.Vector2(
    ((e.clientX - rect.left) / rect.width) * 2 - 1,
    -((e.clientY - rect.top) / rect.height) * 2 + 1,
  );
  raycaster.setFromCamera(mouse, camera);
  const hit = raycaster.intersectObject(mesh)[0];
  if (hit) showTooltip(hit.point, spec.axes);
});
```

`hit.point` is in mesh-local coords — divide by `spec.aspect` to recover real X/Y/Z.

### 2.9 Migration / rollout

1. Add the new Python branch behind a setting: `obb.user.preferences.charting_backend = "d3" | "plotly"` (default `plotly` for one release).
2. Ship the `surface3d` JSON spec only when `charting_backend == "d3"`; otherwise return the existing `OpenBBFigure`.
3. Front-end (`desktop/`) detects `spec.renderer == "d3-three"` and routes to `renderSurface3D`.
4. Once all chart types in `generic_charts.py` have D3 equivalents, flip the default. The Plotly path can stay for one more release for notebook users, then be removed.

### 2.10 Out of scope for v1

- 3D contour lines on the surface (defer; not critical for vol surfaces).
- Smooth shading with computed normals (flat shading matches today's look).
- Animation / morphing between frames.

### 2.11 Acceptance criteria

- [ ] `surface3d(X, Y, Z)` returns a JSON-serializable dict; no `OpenBBFigure` import on this path.
- [ ] Rendering the spec in the desktop app shows a mesh visually equivalent to today's Plotly output for the same input (vol-surface dataset).
- [ ] Orbit / zoom / pan work; hover shows `(x, y, z)` with axis labels.
- [ ] Bundle delta < 200 KB gzipped vs. removing Plotly.
- [ ] Unit tests: triangulation produces the same `indices` as the Plotly path for a fixed seed.
- [ ] Theme toggle (`dark`/`light`) flips background + axis text without re-fetching.
