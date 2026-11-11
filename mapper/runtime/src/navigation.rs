//! Read-only walking navigation. Build/validate on a background worker, never in
//! the aim sampling loop. Geometry comes from the active, agent-radius baked
//! Recast graph; jumps and nonreciprocal/off-mesh links are not walking edges.
use crate::{
    platform::Memory,
    reflection::{Reflection, Result, i32_at, u64_at},
};
use clbb_moba_core::Point;
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, HashSet},
    time::{Duration, Instant},
};

const MAX_PAYLOAD: usize = 8 * 1024 * 1024;
const MAX_TRIANGLES: usize = 100_000;
const MAX_VISIBILITY: usize = 8192;
const ROUTE_BUDGET: Duration = Duration::from_millis(45);

type Checks = Vec<(u64, Vec<u8>)>;
#[derive(Clone, Debug)]
struct Triangle {
    vertices: [usize; 3],
    center: Point,
    min: Point,
    max: Point,
    planes: [[f64; 3]; 3],
    neighbors: Vec<usize>,
}
/// Immutable geometry plus the exact source spans consumed to build it. A match
/// or map replacement revokes it; refresh the whole snapshot on another worker.
#[derive(Clone)]
pub struct NavMesh {
    vertices: Vec<Point>,
    triangles: Vec<Triangle>,
    components: Vec<usize>,
    visibility: Vec<Vec<Point>>,
    checks: Checks,
    pub character_radius: f64,
    pub graph_address: u64,
}
fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite() && p.x.abs() < 100_000. && p.y.abs() < 100_000.
}
fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}
fn area(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}
fn same(a: Point, b: Point) -> bool {
    distance(a, b) < 1e-4
}
fn read_span<M: Memory>(r: &Reflection<M>, address: u64, size: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(size);
    for offset in (0..size).step_by(1024 * 1024) {
        let at = address
            .checked_add(offset as u64)
            .ok_or("Navigation address overflow")?;
        bytes.extend(r.read(at, (size - offset).min(1024 * 1024))?);
    }
    Ok(bytes)
}
fn pin<M: Memory>(
    r: &Reflection<M>,
    checks: &mut Checks,
    address: u64,
    size: usize,
) -> Result<Vec<u8>> {
    if address == 0 || size > MAX_PAYLOAD {
        return Err("Navigation source bounds invalid".into());
    }
    let bytes = read_span(r, address, size)?;
    checks.push((address, bytes.clone()));
    Ok(bytes)
}
fn owner<M: Memory>(r: &Reflection<M>, checks: &mut Checks, address: u64) -> Result<u64> {
    let value = u64_at(&pin(r, checks, address, 8)?, 0);
    if value == 0 {
        return Err("Navigation owner is absent".into());
    }
    Ok(value)
}
fn link<M: Memory>(
    r: &mut Reflection<M>,
    checks: &mut Checks,
    object: u64,
    name: &str,
) -> Result<u64> {
    let address = r.field(object, name)?;
    owner(r, checks, address)
}
fn array<M: Memory>(
    r: &Reflection<M>,
    checks: &mut Checks,
    address: u64,
    stride: usize,
    limit: usize,
    budget: &mut usize,
) -> Result<Vec<u8>> {
    if address == 0 {
        return Err("Navigation array is absent".into());
    }
    let header = r.read(address, 32)?;
    let count = u64_at(&header, 24);
    if count > limit as u64 {
        return Err("Navigation array exceeds its limit".into());
    }
    let size = (count as usize)
        .checked_mul(stride)
        .and_then(|n| n.checked_add(32))
        .ok_or("Navigation array overflow")?;
    *budget = budget
        .checked_add(size)
        .ok_or("Navigation snapshot overflow")?;
    if *budget > MAX_PAYLOAD {
        return Err("Navigation snapshot exceeds 8 MiB".into());
    }
    let bytes = pin(r, checks, address, size)?;
    if bytes[..32] != header {
        return Err("Navigation array changed during read".into());
    }
    Ok(bytes)
}
fn field_offset<M: Memory>(r: &mut Reflection<M>, object: u64, name: &str) -> Result<usize> {
    let address = r.field(object, name)?;
    let offset = address
        .checked_sub(object)
        .ok_or("Navigation field is not an instance field")?;
    if offset > 65536 {
        return Err("Navigation field offset invalid".into());
    }
    Ok(offset as usize)
}
impl NavMesh {
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }
    /// Discover and capture the complete graph. Every owner, list header, flag,
    /// index, vertex and connection byte is read a second time before use.
    pub fn discover<M: Memory>(r: &mut Reflection<M>) -> Result<Self> {
        r.discover(&["PathFindingCS", "GamePathFinding"])?;
        let mut checks = Vec::new();
        let mut budget = 0;
        let path_slot = r.static_field("PathFindingCS", "_instance")?;
        let game_slot = r.static_field("GamePathFinding", "Instance")?;
        let path = owner(r, &mut checks, path_slot)?;
        let game = owner(r, &mut checks, game_slot)?;
        if owner(r, &mut checks, path)? != r.named["PathFindingCS"]
            || owner(r, &mut checks, game)? != r.named["GamePathFinding"]
        {
            return Err("Navigation instance class changed".into());
        }
        if link(r, &mut checks, game, "pathFinding")? != path {
            return Err("Navigation instance mismatch".into());
        }
        let dynamic = link(r, &mut checks, path, "m_DynamicBlock")?;
        owner(r, &mut checks, dynamic)?;
        let graph = link(r, &mut checks, dynamic, "graph")?;
        let graph_class = owner(r, &mut checks, graph)?;
        if r.class(graph_class)?.name != "RecastGraph" {
            return Err("Unsupported walking graph".into());
        }
        for (object, name, size) in [
            (path, "_bPathInvalid", 1),
            (game, "m_useGridCollision", 1),
            (graph, "graphIndex", 4),
        ] {
            let address = r.field(object, name)?;
            if pin(r, &mut checks, address, size)?.iter().any(|n| *n != 0) {
                return Err("Walking graph is inactive".into());
            }
        }
        let radius_at = r.field(graph, "characterRadius")?;
        let radius = f64::from_le_bytes(pin(r, &mut checks, radius_at, 8)?.try_into().unwrap());
        if !radius.is_finite() || radius <= 0. || radius >= 10. {
            return Err("Invalid navigation character radius".into());
        }
        let list = link(r, &mut checks, graph, "tiles")?;
        owner(r, &mut checks, list)?;
        let items = link(r, &mut checks, list, "_items")?;
        let count_at = r.field(list, "_size")?;
        let count = i32_at(&pin(r, &mut checks, count_at, 4)?, 0);
        let version = r.field(list, "_version")?;
        pin(r, &mut checks, version, 4)?;
        if !(1..=4096).contains(&count) {
            return Err("Invalid navigation tile count".into());
        }
        let tile_array = array(r, &mut checks, items, 8, 4096, &mut budget)?;
        let tile_objects: Vec<_> = tile_array[32..]
            .chunks_exact(8)
            .take(count as usize)
            .map(|b| u64_at(b, 0))
            .collect();
        if tile_objects.len() != count as usize
            || tile_objects.contains(&0)
            || tile_objects.iter().copied().collect::<HashSet<_>>().len() != tile_objects.len()
        {
            return Err("Navigation tile list is incomplete or ambiguous".into());
        }
        let tile_class = r.pointer(tile_objects[0])?;
        if r.class(tile_class)?.name != "NavmeshTile" {
            return Err("Unknown navigation tile class".into());
        }
        let tile_offsets = [
            field_offset(r, tile_objects[0], "tris")?,
            field_offset(r, tile_objects[0], "verts")?,
            field_offset(r, tile_objects[0], "nodes")?,
        ];
        let tile_size = tile_offsets.iter().max().unwrap() + 8;
        struct Tile {
            object: u64,
            indices: Vec<u8>,
            vertices: Vec<u8>,
            nodes: Vec<u64>,
        }
        let mut tiles = Vec::new();
        let mut all_nodes = Vec::new();
        for object in tile_objects {
            let raw = pin(r, &mut checks, object, tile_size)?;
            if u64_at(&raw, 0) != tile_class {
                return Err("Navigation tile owner changed".into());
            }
            let indices = array(
                r,
                &mut checks,
                u64_at(&raw, tile_offsets[0]),
                4,
                300000,
                &mut budget,
            )?;
            let vertices = array(
                r,
                &mut checks,
                u64_at(&raw, tile_offsets[1]),
                12,
                100000,
                &mut budget,
            )?;
            let node_array = array(
                r,
                &mut checks,
                u64_at(&raw, tile_offsets[2]),
                8,
                100000,
                &mut budget,
            )?;
            let nodes: Vec<_> = node_array[32..]
                .chunks_exact(8)
                .map(|b| u64_at(b, 0))
                .collect();
            if indices.len() - 32 != nodes.len() * 12 {
                return Err("Navigation triangle/node counts differ".into());
            }
            all_nodes.extend_from_slice(&nodes);
            tiles.push(Tile {
                object,
                indices,
                vertices,
                nodes,
            });
        }
        if all_nodes.is_empty()
            || all_nodes.len() > MAX_TRIANGLES
            || all_nodes.contains(&0)
            || all_nodes.iter().copied().collect::<HashSet<_>>().len() != all_nodes.len()
        {
            return Err("Invalid or duplicated navigation nodes".into());
        }
        let node_class = r.pointer(all_nodes[0])?;
        if r.class(node_class)?.name != "TriangleMeshNode" {
            return Err("Unknown navigation node class".into());
        }
        let names = [
            "tile",
            "vertex0",
            "vertex1",
            "vertex2",
            "flags",
            "<connections>k__BackingField",
        ];
        let offsets: Vec<_> = names
            .iter()
            .map(|name| field_offset(r, all_nodes[0], name))
            .collect::<Result<_>>()?;
        let node_size = offsets.iter().max().unwrap() + 8;
        let mut records = HashMap::new();
        for object in all_nodes {
            let raw = pin(r, &mut checks, object, node_size)?;
            if u64_at(&raw, 0) != node_class {
                return Err("Navigation node owner changed".into());
            }
            let connections = array(r, &mut checks, u64_at(&raw, offsets[5]), 8, 64, &mut budget)?;
            let connected: Vec<_> = connections[32..]
                .chunks_exact(8)
                .map(|b| u64_at(b, 0))
                .collect();
            records.insert(object, (raw, connected));
        }
        let mut vertices = Vec::new();
        let mut indices = HashMap::<[i32; 3], usize>::new();
        let mut triangles = Vec::new();
        let mut nodes = HashMap::new();
        let mut kept = Vec::new();
        for tile in tiles {
            let points: Vec<[i32; 3]> = tile.vertices[32..]
                .chunks_exact(12)
                .map(|b| [i32_at(b, 0), i32_at(b, 4), i32_at(b, 8)])
                .collect();
            for (i, node) in tile.nodes.iter().copied().enumerate() {
                let raw = &records[&node].0;
                if u64_at(raw, offsets[0]) != tile.object {
                    return Err("Navigation node tile mismatch".into());
                }
                let local: [i32; 3] = std::array::from_fn(|n| i32_at(raw, offsets[n + 1]));
                if local.iter().enumerate().any(|(n, v)| {
                    *v != i32_at(&tile.indices, 32 + i * 12 + n * 4)
                        || *v < 0
                        || *v as usize >= points.len()
                }) {
                    return Err("Navigation node vertex mismatch".into());
                }
                if i32_at(raw, offsets[4]) as u32 & 1 == 0 {
                    continue;
                }
                let mut face = [0; 3];
                for (n, v) in local.iter().enumerate() {
                    let point = points[*v as usize];
                    if point.iter().any(|v| v.unsigned_abs() > 100_000_000) {
                        return Err("Navigation vertex out of bounds".into());
                    }
                    face[n] = *indices.entry(point).or_insert_with(|| {
                        let id = vertices.len();
                        vertices.push(Point::new(point[0] as f64 / 1000., point[2] as f64 / 1000.));
                        id
                    });
                }
                nodes.insert(node, triangles.len());
                triangles.push(face);
                kept.push(node);
            }
        }
        if triangles.is_empty() {
            return Err("No walkable navigation triangles".into());
        }
        let neighbors: Vec<Vec<usize>> = kept
            .iter()
            .map(|node| {
                records[node]
                    .1
                    .iter()
                    .filter_map(|n| nodes.get(n).copied())
                    .collect()
            })
            .collect();
        let mut mesh = Self::from_geometry(vertices, triangles, neighbors, radius)?;
        mesh.graph_address = graph;
        mesh.checks = checks;
        mesh.validate(r)?;
        Ok(mesh)
    }
    /// Worker-side validation includes geometry and connections, not just a
    /// singleton address. Never call this full scan inside cursor processing.
    pub fn validate<M: Memory>(&self, r: &Reflection<M>) -> Result<()> {
        for (address, expected) in self.checks.iter().rev() {
            if read_span(r, *address, expected.len())? != *expected {
                return Err("Navigation map changed; rebuild required".into());
            }
        }
        Ok(())
    }
    fn from_geometry(
        vertices: Vec<Point>,
        faces: Vec<[usize; 3]>,
        neighbors: Vec<Vec<usize>>,
        radius: f64,
    ) -> Result<Self> {
        if vertices.len() < 3
            || vertices.len() > 100000
            || faces.is_empty()
            || faces.len() > MAX_TRIANGLES
            || neighbors.len() != faces.len()
            || vertices.iter().any(|p| !finite(*p))
        {
            return Err("Invalid navigation geometry bounds".into());
        }
        let mut triangles = Vec::with_capacity(faces.len());
        for (i, face) in faces.into_iter().enumerate() {
            if face.iter().any(|v| *v >= vertices.len()) {
                return Err("Navigation vertex index out of bounds".into());
            }
            let [a, b, c] = face.map(|v| vertices[v]);
            let orientation = area(a, b, c);
            if orientation.abs() < 1e-8 {
                return Err("Degenerate navigation triangle".into());
            }
            if neighbors[i]
                .iter()
                .any(|n| *n >= neighbors.len() || *n == i)
            {
                return Err("Invalid navigation adjacency".into());
            }
            let sign = orientation.signum();
            let mut planes = [[0.; 3]; 3];
            for k in 0..3 {
                let a = vertices[face[k]];
                let b = vertices[face[(k + 1) % 3]];
                let nx = sign * (a.y - b.y);
                let ny = sign * (b.x - a.x);
                planes[k] = [nx, ny, -nx * a.x - ny * a.y];
            }
            triangles.push(Triangle {
                vertices: face,
                center: (a + b + c) / 3.,
                min: Point::new(a.x.min(b.x).min(c.x), a.y.min(b.y).min(c.y)),
                max: Point::new(a.x.max(b.x).max(c.x), a.y.max(b.y).max(c.y)),
                planes,
                neighbors: neighbors[i].clone(),
            });
        }
        for i in 0..triangles.len() {
            let valid: Vec<_> = triangles[i]
                .neighbors
                .iter()
                .copied()
                .filter(|j| {
                    triangles[*j].neighbors.contains(&i)
                        && shared(&triangles[i], &triangles[*j], &vertices)
                })
                .collect();
            triangles[i].neighbors = valid;
        }
        let mut mesh = Self {
            vertices,
            triangles,
            components: Vec::new(),
            visibility: Vec::new(),
            checks: Vec::new(),
            character_radius: radius,
            graph_address: 0,
        };
        mesh.prepare_visibility();
        Ok(mesh)
    }
    fn prepare_visibility(&mut self) {
        self.components = vec![usize::MAX; self.triangles.len()];
        let mut seen = HashSet::new();
        for root in 0..self.triangles.len() {
            if self.components[root] != usize::MAX {
                continue;
            }
            let component = self.visibility.len();
            self.visibility.push(Vec::new());
            let mut queue = vec![root];
            self.components[root] = component;
            let mut q = 0;
            while q < queue.len() {
                let index = queue[q];
                q += 1;
                for v in self.triangles[index].vertices {
                    let point = self.vertices[v];
                    let key = (
                        component,
                        (point.x * 1e6).round() as i64,
                        (point.y * 1e6).round() as i64,
                    );
                    if seen.insert(key) {
                        self.visibility[component].push(point);
                    }
                }
                for n in self.triangles[index].neighbors.iter().copied() {
                    if self.components[n] == usize::MAX {
                        self.components[n] = component;
                        queue.push(n);
                    }
                }
            }
        }
    }
    /// A direct click stops before the first wall. An avoiding click projects a
    /// wall target onto the nearest reachable surface, then takes the Euclidean
    /// visibility path rather than a triangle-center corridor with long detours.
    pub fn route(&self, start: Point, requested: Point, avoid: bool) -> Result<Vec<Point>> {
        let began = Instant::now();
        if !finite(start) || !finite(requested) {
            return Err("Invalid movement target".into());
        }
        let (first, source) = self
            .nearest(start, None)
            .ok_or("Hero outside navigation mesh")?;
        if distance(start, source) > 1.5 {
            return Err("Hero outside the read walking area".into());
        }
        let component = self.components[first];
        if !avoid {
            let fraction = self.clear_fraction(source, requested, component);
            let length = distance(source, requested);
            let safe = if fraction < 1. - 1e-6 && length > 1e-8 {
                (fraction - 0.035 / length).max(0.)
            } else {
                fraction
            };
            let target = source + (requested - source) * safe;
            return Ok(if distance(start, target) > 0.025 {
                vec![target]
            } else {
                vec![]
            });
        }
        let (last, mut goal) = self
            .nearest(requested, Some(component))
            .ok_or("No reachable walking target")?;
        if distance(goal, requested) > 0.015 {
            let inward = self.triangles[last].center - goal;
            let n = distance(inward, Point::default());
            if n > 1e-8 {
                goal = goal + inward * (0.035_f64.min(n * 0.25) / n);
            }
        }
        if self.clear_fraction(source, goal, component) >= 1. - 1e-6 {
            return Ok(if distance(start, goal) > 0.025 {
                vec![goal]
            } else {
                vec![]
            });
        }
        let mut path = self.visibility_path(source, goal, component, began)?;
        self.offset_corners(source, component, &mut path, began)?;
        Ok(path)
    }
    fn nearest(&self, p: Point, component: Option<usize>) -> Option<(usize, Point)> {
        let mut best = None;
        let mut best_distance = f64::INFINITY;
        for (i, t) in self.triangles.iter().enumerate() {
            if component.is_some_and(|c| self.components[i] != c) {
                continue;
            }
            let q = self.closest(p, t);
            let distance = distance(q, p);
            if distance < best_distance {
                best = Some((i, q));
                best_distance = distance;
            }
            if distance < 1e-8 {
                break;
            }
        }
        best
    }
    fn closest(&self, p: Point, t: &Triangle) -> Point {
        if t.planes
            .iter()
            .all(|[x, y, c]| x * p.x + y * p.y + c >= -1e-8)
        {
            return p;
        }
        let mut best = self.vertices[t.vertices[0]];
        let mut best_distance = f64::INFINITY;
        for k in 0..3 {
            let a = self.vertices[t.vertices[k]];
            let b = self.vertices[t.vertices[(k + 1) % 3]];
            let delta = b - a;
            let n = delta.x * delta.x + delta.y * delta.y;
            let q = if n < 1e-12 {
                a
            } else {
                a + delta * (((p.x - a.x) * delta.x + (p.y - a.y) * delta.y) / n).clamp(0., 1.)
            };
            let d = distance(p, q);
            if d < best_distance {
                best = q;
                best_distance = d;
            }
        }
        best
    }
    fn clear_fraction(&self, from: Point, to: Point, component: usize) -> f64 {
        let delta = to - from;
        let mut intervals = Vec::with_capacity(64);
        let min = Point::new(from.x.min(to.x) - 1e-7, from.y.min(to.y) - 1e-7);
        let max = Point::new(from.x.max(to.x) + 1e-7, from.y.max(to.y) + 1e-7);
        for (i, t) in self.triangles.iter().enumerate() {
            if self.components[i] != component
                || t.max.x < min.x
                || t.min.x > max.x
                || t.max.y < min.y
                || t.min.y > max.y
            {
                continue;
            }
            let mut low = 0_f64;
            let mut high = 1_f64;
            let mut valid = true;
            for [x, y, c] in t.planes {
                let origin = x * from.x + y * from.y + c;
                let slope = x * delta.x + y * delta.y;
                if slope.abs() < 1e-12 {
                    if origin < -1e-8 {
                        valid = false;
                        break;
                    }
                } else {
                    let cut = (-1e-9 - origin) / slope;
                    if slope > 0. {
                        low = low.max(cut);
                    } else {
                        high = high.min(cut);
                    }
                }
                if low > high + 1e-9 {
                    valid = false;
                    break;
                }
            }
            if valid && high >= 0. && low <= 1. {
                intervals.push((low.max(0.), high.min(1.)));
            }
        }
        intervals.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut end = 0_f64;
        for (low, high) in intervals {
            if low > end + 1e-7 {
                break;
            }
            end = end.max(high);
            if end >= 1. {
                return 1.;
            }
        }
        end.clamp(0., 1.)
    }
    fn visibility_path(
        &self,
        source: Point,
        goal: Point,
        component: usize,
        began: Instant,
    ) -> Result<Vec<Point>> {
        let fixed = self
            .visibility
            .get(component)
            .ok_or("No connected walking area")?;
        if fixed.len() > MAX_VISIBILITY {
            return Err("Navigation visibility graph too complex".into());
        }
        let mut points = vec![source, goal];
        points.extend(
            fixed
                .iter()
                .copied()
                .filter(|p| !same(*p, source) && !same(*p, goal)),
        );
        let mut cost = vec![f64::INFINITY; points.len()];
        let mut parent = vec![usize::MAX; points.len()];
        let mut closed = vec![false; points.len()];
        let mut open = BinaryHeap::new();
        cost[0] = 0.;
        closed[0] = true;
        let enqueue = |here: usize,
                       open: &mut BinaryHeap<Pending>,
                       closed: &[bool],
                       cost: &[f64]|
         -> Result<()> {
            for next in 0..points.len() {
                if closed[next] {
                    continue;
                }
                if next & 127 == 0
                    && (began.elapsed() > Duration::from_millis(40) || open.len() > 200000)
                {
                    return Err("Navigation search complexity budget exceeded".into());
                }
                let candidate = cost[here] + distance(points[here], points[next]);
                open.push(Pending {
                    priority: candidate + distance(points[next], goal),
                    here: next,
                    from: here,
                    cost: candidate,
                });
            }
            Ok(())
        };
        enqueue(0, &mut open, &closed, &cost)?;
        let mut casts = 0;
        let mut expanded = 0;
        while let Some(edge) = open.pop() {
            if closed[edge.here] {
                continue;
            }
            if began.elapsed() > Duration::from_millis(40) || casts > 30000 {
                return Err("Navigation search time budget exceeded".into());
            }
            casts += 1;
            if self.clear_fraction(points[edge.from], points[edge.here], component) < 1. - 1e-6 {
                continue;
            }
            closed[edge.here] = true;
            cost[edge.here] = edge.cost;
            parent[edge.here] = edge.from;
            if edge.here == 1 {
                break;
            }
            expanded += 1;
            if expanded > 2048 {
                return Err("Navigation search expansion budget exceeded".into());
            }
            enqueue(edge.here, &mut open, &closed, &cost)?;
        }
        if parent[1] == usize::MAX {
            return Err("No walking route to target".into());
        }
        let mut path = Vec::new();
        let mut here = 1;
        while here != 0 {
            if here >= points.len() || path.len() > points.len() {
                return Err("Invalid navigation parent chain".into());
            }
            path.push(points[here]);
            here = parent[here];
        }
        path.reverse();
        Ok(path)
    }
    fn strictly_inside(&self, p: Point, component: usize) -> bool {
        self.triangles.iter().enumerate().any(|(i, t)| {
            self.components[i] == component
                && p.x >= t.min.x
                && p.x <= t.max.x
                && p.y >= t.min.y
                && p.y <= t.max.y
                && t.planes
                    .iter()
                    .all(|[x, y, c]| x * p.x + y * p.y + c > 1e-7 * x.hypot(*y))
        })
    }
    fn offset_corners(
        &self,
        source: Point,
        component: usize,
        path: &mut [Point],
        began: Instant,
    ) -> Result<()> {
        for i in 0..path.len().saturating_sub(1) {
            if began.elapsed() > ROUTE_BUDGET {
                return Err("Navigation corner budget exceeded".into());
            }
            let previous = if i == 0 { source } else { path[i - 1] };
            let point = path[i];
            let next = path[i + 1];
            if self.strictly_inside(point, component) {
                continue;
            }
            let maximum = 0.025_f64
                .min(distance(previous, point) * 0.2)
                .min(distance(point, next) * 0.2);
            let mut best = None;
            let mut best_length = f64::INFINITY;
            for attempt in 0..4 {
                let radius = maximum / 2_f64.powi(attempt);
                if radius < 1e-6 {
                    break;
                }
                for k in 0..48 {
                    if began.elapsed() > ROUTE_BUDGET {
                        return Err("Navigation corner budget exceeded".into());
                    }
                    let angle = std::f64::consts::TAU * (k as f64 + 0.37) / 48.;
                    let p = point + Point::new(angle.cos(), angle.sin()) * radius;
                    if !self.strictly_inside(p, component)
                        || self.clear_fraction(previous, p, component) < 1. - 1e-6
                        || self.clear_fraction(p, next, component) < 1. - 1e-6
                    {
                        continue;
                    }
                    let length = distance(previous, p) + distance(p, next);
                    if length < best_length {
                        best = Some(p);
                        best_length = length;
                    }
                }
                if best.is_some() {
                    break;
                }
            }
            path[i] =
                best.ok_or("Navigation route touches a boundary too narrow for safe movement")?;
        }
        Ok(())
    }
}
fn shared(a: &Triangle, b: &Triangle, vertices: &[Point]) -> bool {
    let mut matches = Vec::with_capacity(3);
    for x in a.vertices {
        for y in b.vertices {
            let p = vertices[x];
            if same(p, vertices[y]) && !matches.iter().any(|q| same(*q, p)) {
                matches.push(p);
            }
        }
    }
    matches.len() == 2
}
#[derive(Clone, Copy, Debug)]
struct Pending {
    priority: f64,
    here: usize,
    from: usize,
    cost: f64,
}
impl PartialEq for Pending {
    fn eq(&self, other: &Self) -> bool {
        self.priority.total_cmp(&other.priority) == Ordering::Equal
            && self.here == other.here
            && self.from == other.from
    }
}
impl Eq for Pending {}
impl PartialOrd for Pending {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Pending {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .priority
            .total_cmp(&self.priority)
            .then(other.here.cmp(&self.here))
            .then(other.from.cmp(&self.from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Unit squares triangulated on a grid, omitting selected squares for walls.
    fn grid(width: usize, height: usize, blocked: &[(usize, usize)]) -> NavMesh {
        let mut vertices = Vec::new();
        for y in 0..=height {
            for x in 0..=width {
                vertices.push(Point::new(x as f64, y as f64));
            }
        }
        let mut faces = Vec::new();
        for y in 0..height {
            for x in 0..width {
                if blocked.contains(&(x, y)) {
                    continue;
                }
                let a = y * (width + 1) + x;
                let b = a + 1;
                let c = a + width + 1;
                let d = c + 1;
                faces.push([a, b, d]);
                faces.push([a, d, c]);
            }
        }
        let mut neighbors = vec![Vec::new(); faces.len()];
        for i in 0..faces.len() {
            for j in i + 1..faces.len() {
                if faces[i].iter().filter(|v| faces[j].contains(v)).count() == 2 {
                    neighbors[i].push(j);
                    neighbors[j].push(i);
                }
            }
        }
        NavMesh::from_geometry(vertices, faces, neighbors, 0.4).unwrap()
    }
    #[test]
    fn open_area_is_always_a_single_straight_leg() {
        let mesh = grid(6, 5, &[]);
        let start = Point::new(0.1, 0.2);
        let target = Point::new(5.8, 4.7);
        for avoid in [false, true] {
            assert_eq!(mesh.route(start, target, avoid).unwrap(), vec![target]);
        }
    }
    #[test]
    fn direct_click_stops_before_wall_without_turning() {
        let mesh = grid(5, 3, &[(2, 1)]);
        let start = Point::new(0.5, 1.5);
        let end = mesh.route(start, Point::new(4.5, 1.5), false).unwrap()[0];
        assert!((end.x - 1.965).abs() < 1e-6);
        assert_eq!(end.y, 1.5);
    }
    #[test]
    fn smart_route_uses_shortest_side_and_never_crosses_wall() {
        let mesh = grid(5, 4, &[(2, 1), (2, 2)]);
        let start = Point::new(0.5, 0.7);
        let target = Point::new(4.5, 1.4);
        let path = mesh.route(start, target, true).unwrap();
        assert!(path.len() >= 2);
        assert_eq!(*path.last().unwrap(), target);
        let mut p = start;
        let mut length = 0.;
        for q in path {
            assert!(mesh.clear_fraction(p, q, mesh.components[0]) > 1. - 1e-6);
            length += distance(p, q);
            p = q;
        }
        assert!(length < 4.5, "Unnecessary detour: {length}");
    }
    #[test]
    fn click_inside_wall_projects_and_finishes_on_reachable_boundary() {
        let mesh = grid(5, 3, &[(2, 1)]);
        let start = Point::new(0.5, 1.5);
        let requested = Point::new(2.2, 1.5);
        let path = mesh.route(start, requested, true).unwrap();
        let end = *path.last().unwrap();
        assert!(end.x < 2. && end.x > 1.94);
        assert!(distance(end, requested) < 0.25);
        assert!(
            mesh.nearest(end, Some(mesh.components[0]))
                .is_some_and(|(_, q)| distance(q, end) < 1e-8)
        );
    }
    #[test]
    fn disconnected_island_does_not_make_a_walk_link() {
        let mesh = grid(5, 1, &[(2, 0)]);
        let start = Point::new(0.5, 0.5);
        let path = mesh.route(start, Point::new(4.5, 0.5), true).unwrap();
        assert!(path.last().unwrap().x < 2.);
    }
    #[test]
    fn diagonal_vertex_contact_is_not_a_passage() {
        let mesh = grid(2, 2, &[(1, 0), (0, 1)]);
        let path = mesh
            .route(Point::new(0.5, 0.5), Point::new(1.5, 1.5), true)
            .unwrap();
        assert!(path.last().unwrap().x <= 1. && path.last().unwrap().y <= 1.);
    }
    #[test]
    fn nonreciprocal_and_offmesh_edges_are_rejected() {
        let v = vec![
            Point::new(0., 0.),
            Point::new(1., 0.),
            Point::new(0., 1.),
            Point::new(1., 1.),
            Point::new(3., 3.),
            Point::new(4., 3.),
            Point::new(3., 4.),
        ];
        let mesh = NavMesh::from_geometry(
            v,
            vec![[0, 1, 2], [1, 3, 2], [4, 5, 6]],
            vec![vec![1, 2], vec![], vec![0]],
            0.4,
        )
        .unwrap();
        assert!(mesh.triangles.iter().all(|t| t.neighbors.is_empty()));
    }
    #[test]
    fn source_change_revokes_even_an_unchanged_graph_address() {
        use std::cell::RefCell;
        struct MutableMemory(RefCell<Vec<u8>>);
        impl Memory for MutableMemory {
            fn regions(&self) -> std::result::Result<Vec<crate::platform::Region>, String> {
                Ok(vec![])
            }
            fn read(&self, address: u64, length: usize) -> Result<Vec<u8>> {
                self.0
                    .borrow()
                    .get(address as usize..address as usize + length)
                    .map(|v| v.to_vec())
                    .ok_or("Invalid test memory".into())
            }
            fn read_into(&self, address: u64, bytes: &mut [u8]) -> Result<()> {
                bytes.copy_from_slice(&self.read(address, bytes.len())?);
                Ok(())
            }
        }
        let memory = MutableMemory(RefCell::new(vec![0; 64]));
        let reflection = Reflection::new(memory).unwrap();
        let mut mesh = grid(2, 2, &[]);
        mesh.checks = vec![(16, vec![0; 8]), (40, vec![0; 4])];
        assert!(mesh.validate(&reflection).is_ok());
        reflection.memory.0.borrow_mut()[43] = 1; // e.g. an in-place node flag/link change
        assert!(mesh.validate(&reflection).is_err());
        reflection.memory.0.borrow_mut()[43] = 0;
        reflection.memory.0.borrow_mut()[16] = 1;
        assert!(mesh.validate(&reflection).is_err());
    }
    #[test]
    fn invalid_geometry_and_cursor_are_rejected() {
        let mesh = grid(2, 2, &[]);
        assert!(
            mesh.route(Point::new(f64::NAN, 0.), Point::new(1., 1.), true)
                .is_err()
        );
        assert!(
            mesh.route(Point::new(99., 99.), Point::new(1., 1.), true)
                .is_err()
        );
        assert!(
            NavMesh::from_geometry(
                vec![Point::new(0., 0.); 3],
                vec![[0, 1, 2]],
                vec![vec![]],
                0.4
            )
            .is_err()
        );
    }
}
