//! The mechanism: how cogs touch, the solver that gives every cog its speed, seats, snapping, clearance, resizing
//! with meshes kept, and the plate's angle, start and 0 (sim.js, DESIGN.md "The start and 0").

use crate::model::*;
use crate::world::World;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Contact {
    Axle,
    Mesh,
    Jam,
}

/// How two cogs touch: on one axle, teeth in mesh, overlapping (a jam), or apart.
pub fn contact(s: &Settings, a: &Cog, b: &Cog) -> Option<Contact> {
    let d = hypot(a.x - b.x, a.y - b.y);
    if d <= s.coax_tol {
        return Some(Contact::Axle);
    }
    if a.layer != b.layer {
        return None;
    }
    let gap = d - a.radius() - b.radius();
    if gap.abs() <= s.mesh_tol { Some(Contact::Mesh) } else if gap < 0.0 { Some(Contact::Jam) } else { None }
}

/// Where a reader lies on a disc, as a seat.
pub fn seat_of(r: &Reader, c: &Cog) -> Seat {
    let a = ((r.y1 + r.y2) / 2.0 - c.y).atan2((r.x1 + r.x2) / 2.0 - c.x);
    let rr = c.radius();
    Seat { ang: a, f0: hypot(r.x1 - c.x, r.y1 - c.y) / rr, f1: hypot(r.x2 - c.x, r.y2 - c.y) / rr }
}
pub fn seat_reader(r: &mut Reader, c: &Cog) {
    r.on = Some(c.id);
    r.seat = Some(seat_of(r, c));
}
/// Put a seated reader where its seat says on its disc.
pub fn place_seated(r: &mut Reader, c: &Cog) {
    let Some(q) = r.seat else { return };
    let rr = c.radius();
    let (ux, uy) = (q.ang.cos(), q.ang.sin());
    let (x1, y1, x2, y2) = (c.x + ux * q.f0 * rr, c.y + uy * q.f0 * rr, c.x + ux * q.f1 * rr, c.y + uy * q.f1 * rr);
    if (x1 - r.x1).abs() + (y1 - r.y1).abs() + (x2 - r.x2).abs() + (y2 - r.y2).abs() > 1e-7 {
        r.x1 = x1;
        r.y1 = y1;
        r.x2 = x2;
        r.y2 = y2;
    }
}

/// sim.js interlock: set c's tooth offset so its teeth sit between partner's where they meet.
pub fn interlock(c: &mut Cog, partner: &Cog) {
    let phi = (c.y - partner.y).atan2(c.x - partner.x);
    let tooth_on_partner = (phi - partner.th - partner.tooth) * partner.n / TAU;
    c.tooth = wrap(phi + std::f64::consts::PI - (0.5 - tooth_on_partner) * TAU / c.n - c.th);
}

impl World {
    /// motorRpm: a motor's speed, from the tempo lock when there is one.
    pub fn motor_rpm(&self, c: &Cog) -> f64 {
        match c.motor {
            None => 0.0,
            Some(m) => {
                if self.s.bpm != 0.0 && m.revs != 0.0 { m.revs * self.s.bpm / self.s.beats } else { m.rpm }
            }
        }
    }
    pub fn contact(&self, a: usize, b: usize) -> Option<Contact> {
        contact(&self.s, &self.cogs[a], &self.cogs[b])
    }
    /// Cogs on the axle at (x, y).
    pub fn stack_at(&self, x: f64, y: f64) -> impl Iterator<Item = usize> + '_ {
        let tol = self.s.coax_tol;
        self.cogs.iter().enumerate().filter(move |(_, o)| hypot(o.x - x, o.y - y) <= tol).map(|(i, _)| i)
    }
    /// Nothing else turns with this cog: no other cog touches it and no belt holds it.
    pub fn loose(&self, c: usize) -> bool {
        (0..self.cogs.len()).all(|o| o == c || self.contact(c, o).is_none()) && self.belts.iter().all(|b| b.a_ix != c && b.b_ix != c)
    }
    /// What is drawn: a cog's angle at the heard time.
    pub fn render_th(&self, c: usize) -> f64 {
        let c = &self.cogs[c];
        c.th - c.w * (self.clock.sim_t - self.clock.view)
    }

    /// sim.js solve: seat readers, then give every cog its ratio to its train's root and its speed from the train's
    /// motor (or the hand on the speed knob); a train that overlaps, disagrees with itself or has two motors jams.
    pub fn solve(&mut self) {
        self.relink();
        for r in &mut self.readers {
            if r.on.is_some() {
                match r.on_ix {
                    Some(i) if self.cogs[i].track.is_some() => place_seated(r, &self.cogs[i]),
                    _ => {
                        r.on = None;
                        r.on_ix = None;
                    }
                }
            }
        }
        let n = self.cogs.len();
        let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        for c in &mut self.cogs {
            c.jam = false;
            c.k = None;
            c.w = 0.0;
        }
        for i in 0..n {
            for j in i + 1..n {
                match self.contact(i, j) {
                    Some(Contact::Axle) => {
                        adj[i].push((j, 1.0));
                        adj[j].push((i, 1.0));
                    }
                    Some(Contact::Mesh) => {
                        let (a, b) = (self.cogs[i].n, self.cogs[j].n);
                        adj[i].push((j, -a / b));
                        adj[j].push((i, -b / a));
                    }
                    Some(Contact::Jam) => {
                        self.cogs[i].jam = true;
                        self.cogs[j].jam = true;
                    }
                    None => {}
                }
            }
        }
        for b in &self.belts {
            let s = if b.cross { -1.0 } else { 1.0 };
            let (na, nb) = (self.cogs[b.a_ix].n, self.cogs[b.b_ix].n);
            adj[b.a_ix].push((b.b_ix, s * na / nb));
            adj[b.b_ix].push((b.a_ix, s * nb / na));
        }
        let mut comp = Vec::new();
        for root in 0..n {
            if self.cogs[root].k.is_some() {
                continue;
            }
            self.cogs[root].k = Some(1.0);
            comp.clear();
            comp.push(root);
            let mut jam = false;
            let mut q = 0;
            while q < comp.len() {
                let a = comp[q];
                let ka = self.cogs[a].k.unwrap();
                for &(b, ratio) in &adj[a] {
                    let k = ka * ratio;
                    match self.cogs[b].k {
                        None => {
                            self.cogs[b].k = Some(k);
                            comp.push(b);
                        }
                        Some(kb) => {
                            if (kb - k).abs() > 1e-9 * k.abs() {
                                jam = true;
                            }
                        }
                    }
                }
                q += 1;
            }
            if comp.iter().any(|&c| self.cogs[c].jam) {
                jam = true;
            }
            let mut motors = comp.iter().copied().filter(|&c| self.cogs[c].motor.is_some());
            let driver = motors.next();
            if motors.next().is_some() {
                jam = true;
            }
            let hand = comp.iter().copied().find(|&c| self.cogs[c].scrub.is_some());
            let w_root = if let Some(h) = hand {
                self.cogs[h].scrub.unwrap() * TAU / 60.0 / self.cogs[h].k.unwrap()
            } else if let Some(d) = driver {
                self.motor_rpm(&self.cogs[d]) * TAU / 60.0 / self.cogs[d].k.unwrap()
            } else {
                0.0
            };
            for &c in &comp {
                let cg = &mut self.cogs[c];
                cg.jam = jam;
                cg.w = if jam { 0.0 } else { cg.k.unwrap() * w_root };
                cg.driver = driver;
            }
        }
        self.field_slots();
        self.refresh_probes();
    }

    /// sim.js snap: pull a cog into mesh with the nearest cog on its layer, within the snap tolerance.
    pub fn snap(&mut self, c: usize) {
        let mut best = None;
        let mut best_gap = f64::INFINITY;
        let cc = &self.cogs[c];
        for (i, o) in self.cogs.iter().enumerate() {
            if i == c || o.layer != cc.layer {
                continue;
            }
            let d = hypot(cc.x - o.x, cc.y - o.y);
            if d <= self.s.coax_tol {
                continue;
            }
            let g = (d - o.radius() - cc.radius()).abs();
            if g < best_gap {
                best_gap = g;
                best = Some(i);
            }
        }
        let Some(b) = best else { return };
        if best_gap > self.s.snap_tol {
            return;
        }
        let (bx, by, br) = (self.cogs[b].x, self.cogs[b].y, self.cogs[b].radius());
        let cc = &mut self.cogs[c];
        let d = {
            let d = hypot(cc.x - bx, cc.y - by);
            if d == 0.0 { 1.0 } else { d }
        };
        let r = br + cc.radius();
        cc.x = bx + (cc.x - bx) / d * r;
        cc.y = by + (cc.y - by) / d * r;
        let partner = self.cogs[b].clone();
        interlock(&mut self.cogs[c], &partner);
    }
    /// stage.js snapClear: snap only where that overlaps nothing; the rest of `stack` rides along with c.
    pub fn snap_clear(&mut self, c: usize, stack: &[usize]) -> bool {
        let (bx, by, tooth) = (self.cogs[c].x, self.cogs[c].y, self.cogs[c].tooth);
        self.snap(c);
        let (ex, ey) = (self.cogs[c].x - bx, self.cogs[c].y - by);
        if ex == 0.0 && ey == 0.0 {
            return false;
        }
        for &o in stack {
            if o != c {
                self.cogs[o].x += ex;
                self.cogs[o].y += ey;
            }
        }
        if !stack.iter().any(|&o| self.clashes(o, stack)) {
            return true;
        }
        for &o in stack {
            if o != c {
                self.cogs[o].x -= ex;
                self.cogs[o].y -= ey;
            }
        }
        let cc = &mut self.cogs[c];
        cc.x = bx;
        cc.y = by;
        cc.tooth = tooth;
        false
    }
    /// Cogs never overlap: c's spot is clear when it misses every other cog on its layer (meshing teeth touch, they do
    /// not overlap) or shares its axle; `skip` holds cogs moving with it.
    pub fn clashes(&self, c: usize, skip: &[usize]) -> bool {
        let cc = &self.cogs[c];
        for (i, o) in self.cogs.iter().enumerate() {
            if i == c || o.layer != cc.layer || skip.contains(&i) {
                continue;
            }
            let d = hypot(cc.x - o.x, cc.y - o.y);
            if d > self.s.coax_tol && d - o.radius() - cc.radius() < -self.s.mesh_tol {
                return true;
            }
        }
        false
    }
    /// How many cogs overlap another.
    pub fn overlaps(&self) -> usize {
        (0..self.cogs.len()).filter(|&c| self.clashes(c, &[])).count()
    }

    /// sim.js clearShift: move cogs rigidly (`ms` holds [cog, x0, y0]) by the wanted offset, or the nearest clear one
    /// (in mesh against one cog, or nestled between two), failing that `last`. Returns the offset used.
    pub fn clear_shift(&mut self, ms: &[(usize, f64, f64)], dx: f64, dy: f64, last: Option<(f64, f64)>) -> (f64, f64) {
        let set: Vec<usize> = ms.iter().map(|m| m.0).collect();
        let at = |w: &mut World, ex: f64, ey: f64| {
            for &(o, x, y) in ms {
                w.cogs[o].x = x + ex;
                w.cogs[o].y = y + ey;
            }
            !ms.iter().any(|&(o, _, _)| w.clashes(o, &set))
        };
        if ms.is_empty() || at(self, dx, dy) {
            return (dx, dy);
        }
        struct Rim {
            x: f64,
            y: f64,
            r: f64,
        }
        let mut rims = Vec::new();
        for &(m, x, y) in ms {
            for (i, o) in self.cogs.iter().enumerate() {
                let mm = &self.cogs[m];
                if !set.contains(&i) && o.layer == mm.layer && hypot(mm.x - o.x, mm.y - o.y) > self.s.coax_tol {
                    rims.push(Rim { x: o.x - x, y: o.y - y, r: o.radius() + mm.radius() });
                }
            }
        }
        let away = match last {
            Some(l) => (dy - l.1).atan2(dx - l.0) + std::f64::consts::PI,
            None => 0.0,
        };
        let mut cand: Vec<(f64, f64)> = Vec::new();
        for r in &rims {
            let (ex, ey) = (dx - r.x, dy - r.y);
            let d = hypot(ex, ey);
            if d >= r.r {
                continue;
            }
            let a = if d > 1e-9 { ey.atan2(ex) } else { away };
            cand.push((r.x + a.cos() * r.r, r.y + a.sin() * r.r));
        }
        let near: Vec<&Rim> = rims.iter().filter(|r| hypot(dx - r.x, dy - r.y) < r.r + 400.0).collect();
        for i in 0..near.len() {
            for j in i + 1..near.len() {
                let (a, b) = (near[i], near[j]);
                let dd = hypot(b.x - a.x, b.y - a.y);
                if dd < 1e-9 || dd > a.r + b.r || dd < (a.r - b.r).abs() {
                    continue;
                }
                let l = (a.r * a.r - b.r * b.r + dd * dd) / (2.0 * dd);
                let h = js_max(0.0, a.r * a.r - l * l).sqrt();
                let (ux, uy) = ((b.x - a.x) / dd, (b.y - a.y) / dd);
                let (px, py) = (a.x + ux * l, a.y + uy * l);
                cand.push((px - uy * h, py + ux * h));
                cand.push((px + uy * h, py - ux * h));
            }
        }
        cand.sort_by(|p, q| {
            hypot(p.0 - dx, p.1 - dy).partial_cmp(&hypot(q.0 - dx, q.1 - dy)).unwrap_or(std::cmp::Ordering::Equal)
        });
        for (ex, ey) in cand {
            if at(self, ex, ey) {
                return (ex, ey);
            }
        }
        if let Some(l) = last {
            at(self, l.0, l.1);
            return l;
        }
        at(self, dx, dy);
        (dx, dy)
    }

    /// A plate's angle is its timing, set only by hand. Turn plate c to angle a as drawn; its teeth stay in the mesh.
    /// On the start, this is its new start.
    pub fn set_phase(&mut self, c: usize, a: f64) {
        let d = a - self.render_th(c);
        let lead = self.clock.sim_t - self.clock.view;
        let loose = self.loose(c);
        let cc = &mut self.cogs[c];
        cc.th = a + cc.w * lead;
        if !loose {
            cc.tooth = wrap(cc.tooth - d);
        }
        if self.clock.at_start {
            cc.th0 = wrap(a);
        }
    }
    /// A plate's 0, pointing exactly right: it stands there now, and starts there from here on.
    pub fn zero_plate(&mut self, c: usize) {
        self.set_phase(c, 0.0);
        self.cogs[c].th0 = 0.0;
    }
    /// A seated reader to its disc's 0 (stage.js zeroSel).
    pub fn zero_reader(&mut self, r: usize) {
        let rd = &mut self.readers[r];
        if let (Some(s), Some(ci)) = (rd.seat.as_mut(), rd.on_ix) {
            s.ang = 0.0;
            place_seated(rd, &self.cogs[ci]);
        }
    }
    /// The reset button: stop, and put every plate on its stored start. The sim comes back to the heard time; the
    /// teeth stay where they are, so every mesh holds. Balls are left as they are.
    pub fn to_start(&mut self) {
        self.clock.paused = true;
        self.clock.at_start = true;
        for c in &mut self.cogs {
            c.scrub = None;
        }
        for i in 0..self.cogs.len() {
            let rt = self.render_th(i);
            let c = &mut self.cogs[i];
            c.tooth = wrap(c.tooth + rt - c.th0);
            c.turns = 0.0;
        }
        self.clock.sim_t = self.clock.view;
        for c in &mut self.cogs {
            c.th = c.th0;
        }
        self.solve();
    }
    /// Pause or play (sim.js setPaused, the machine's part of it). Returns whether play starts from the start, when the
    /// first step should be scheduled so a dot on the line sounds on time.
    pub fn set_paused(&mut self, p: bool) -> bool {
        if p == self.clock.paused {
            return false;
        }
        self.clock.paused = p;
        let from_start = !p && self.clock.at_start;
        if !p {
            self.clock.at_start = false;
        }
        from_start
    }

    /// sim.js resizer: change cog c's tooth count keeping every mesh.
    pub fn resizer(&self, c: usize) -> Resizer {
        let n = self.cogs.len();
        let mut links: Vec<Vec<(usize, Contact)>> = vec![Vec::new(); n];
        for i in 0..n {
            for j in i + 1..n {
                if let Some(h @ (Contact::Axle | Contact::Mesh)) = self.contact(i, j) {
                    links[i].push((j, h));
                    links[j].push((i, h));
                }
            }
        }
        let stack: Vec<usize> = self.stack_at(self.cogs[c].x, self.cogs[c].y).collect();
        let meshed: Vec<usize> = stack.iter()
            .flat_map(|&o| links[o].iter().filter(|l| l.1 == Contact::Mesh).map(|l| l.0))
            .filter(|m| !stack.contains(m))
            .collect();
        let mut anchor = meshed.first().copied();
        if let Some(d) = self.cogs[c].driver {
            if !stack.contains(&d) {
                let mut seen = vec![d];
                let mut q = vec![d];
                let mut i = 0;
                while i < q.len() && !q.iter().any(|o| meshed.contains(o)) {
                    for &(o, _) in &links[q[i]].clone() {
                        if !seen.contains(&o) && !stack.contains(&o) {
                            seen.push(o);
                            q.push(o);
                        }
                    }
                    i += 1;
                }
                if let Some(a) = q.iter().copied().find(|o| meshed.contains(o)) {
                    anchor = Some(a);
                }
            }
        }
        Resizer {
            c,
            anchor,
            n0: self.cogs[c].n,
            links,
            cogs_at: self.cogs.iter().map(|o| (o.x, o.y)).collect(),
            readers_at: self.readers.iter().map(|r| [r.x1, r.y1, r.x2, r.y2]).collect(),
        }
    }

    /// stage.js setTeeth: size a cog within its limits and the ratio snap; a size that would overlap anything is
    /// refused. Returns whether the count changed.
    pub fn set_teeth(&mut self, grow: &Resizer, raw: f64) -> bool {
        let c = grow.c;
        let min = if self.cogs[c].track.is_some() { 12.0 } else { 6.0 };
        let n = self.ratio_snap(grow.anchor, clamp(raw, min, 240.0), min);
        if n == self.cogs[c].n {
            return false;
        }
        let was = self.cogs[c].n;
        let jams = self.overlaps();
        grow.set(self, n);
        if self.overlaps() > jams {
            grow.set(self, was);
            return false;
        }
        true
    }
    /// Ratio snap (S.ratios): only tooth counts in a simple ratio to the anchor; the nearest to the pull wins.
    pub fn ratio_snap(&self, anchor: Option<usize>, n: f64, min: f64) -> f64 {
        const SNAP: [f64; 11] = [1.0 / 4.0, 1.0 / 3.0, 1.0 / 2.0, 2.0 / 3.0, 3.0 / 4.0, 1.0, 4.0 / 3.0, 3.0 / 2.0, 2.0, 3.0, 4.0];
        let (true, Some(a)) = (self.s.ratios != 0.0, anchor) else { return n };
        let an = self.cogs[a].n;
        let ok: Vec<f64> = SNAP.iter().map(|r| an * r).filter(|m| *m == m.trunc() && *m >= min && *m <= 240.0).collect();
        if ok.is_empty() {
            return n;
        }
        ok.iter().copied().fold(ok[0], |b, m| if (m / n).ln().abs() < (b / n).ln().abs() { m } else { b })
    }

    /// stage.js drive: a speed given to any cog goes to its train's motor, scaled by the ratio between them.
    pub fn drive(&mut self, c: usize, rpm: f64) -> bool {
        let m = if self.cogs[c].motor.is_some() { Some(c) } else { self.cogs[c].driver };
        let Some(m) = m else { return false };
        let r = if m == c { rpm } else { rpm * self.cogs[m].k.unwrap_or(1.0) / self.cogs[c].k.unwrap_or(1.0) };
        let revs = if self.s.bpm != 0.0 { r * self.s.beats / self.s.bpm } else { 0.0 };
        self.cogs[m].motor = Some(Motor { rpm: r, revs });
        self.solve();
        true
    }
    /// stage.js retimed: run a change of speed so it takes effect where the cogs are drawn: each cog's phase moves by
    /// its change of speed times the sim's lead over the picture.
    pub fn retimed(&mut self, f: impl FnOnce(&mut World)) {
        let w0: Vec<(u32, f64)> = self.cogs.iter().map(|c| (c.id, c.w)).collect();
        f(self);
        let lead = self.clock.sim_t - self.clock.view;
        if self.clock.paused || !(lead > 0.0 && lead < 1.0) {
            return;
        }
        for c in &mut self.cogs {
            let dw = c.w - w0.iter().find(|x| x.0 == c.id).map(|x| x.1).unwrap_or(c.w);
            if dw != 0.0 {
                let th = c.th + dw * lead;
                let k = (th / TAU).floor();
                c.th = th - k * TAU;
                c.turns += k;
            }
        }
    }
}

/// A cog being resized: made by `World::resizer` when the gesture starts, `set` for each new tooth count.
pub struct Resizer {
    pub c: usize,
    /// The cog whose contact with c stays put.
    pub anchor: Option<usize>,
    n0: f64,
    links: Vec<Vec<(usize, Contact)>>,
    cogs_at: Vec<(f64, f64)>,
    readers_at: Vec<[f64; 4]>,
}
impl Resizer {
    /// The radius whose rim passes through world point q while the contact with the anchor stays put.
    pub fn radius_through(&self, w: &World, qx: f64, qy: f64) -> f64 {
        let p = self.cogs_at[self.c];
        let Some(an) = self.anchor else { return hypot(qx - w.cogs[self.c].x, qy - w.cogs[self.c].y) };
        let a = self.cogs_at[an];
        let d = hypot(p.0 - a.0, p.1 - a.1);
        let (ux, uy) = ((p.0 - a.0) / d, (p.1 - a.1) / d);
        let ra = w.cogs[an].radius();
        let (tx, ty) = (a.0 + ux * ra, a.1 + uy * ra);
        let (qx, qy) = (qx - tx, qy - ty);
        let along = qx * ux + qy * uy;
        if along > 8.0 { (qx * qx + qy * qy) / (2.0 * along) } else { hypot(qx + tx - p.0, qy + ty - p.1) }
    }
    fn carried(&self, w: &World, o: usize) -> Vec<usize> {
        let (ox, oy) = self.cogs_at[o];
        let r = radius_n(if o == self.c { self.n0 } else { w.cogs[o].n });
        (0..self.readers_at.len().min(w.readers.len()))
            .filter(|&i| hypot(self.readers_at[i][0] - ox, self.readers_at[i][1] - oy) < r)
            .collect()
    }
    fn shift(&self, w: &mut World, o: usize, dx: f64, dy: f64) {
        let p = self.cogs_at[o];
        w.cogs[o].x = p.0 + dx;
        w.cogs[o].y = p.1 + dy;
        for r in self.carried(w, o) {
            let q = self.readers_at[r];
            let rd = &mut w.readers[r];
            rd.x1 = q[0] + dx;
            rd.y1 = q[1] + dy;
            rd.x2 = q[2] + dx;
            rd.y2 = q[3] + dy;
        }
    }
    pub fn set(&self, w: &mut World, n: f64) {
        let c = self.c;
        w.cogs[c].n = n;
        for (i, p) in self.cogs_at.iter().enumerate() {
            if i < w.cogs.len() {
                w.cogs[i].x = p.0;
                w.cogs[i].y = p.1;
            }
        }
        for (i, q) in self.readers_at.iter().enumerate() {
            if i < w.readers.len() {
                let r = &mut w.readers[i];
                r.x1 = q[0];
                r.y1 = q[1];
                r.x2 = q[2];
                r.y2 = q[3];
            }
        }
        if let Some(anchor) = self.anchor {
            let mut done = vec![anchor];
            let mut q: Vec<usize> = Vec::new();
            let mv = |w: &mut World, done: &mut Vec<usize>, q: &mut Vec<usize>, o: usize, dx: f64, dy: f64| {
                let (ox, oy) = self.cogs_at[o];
                let stack: Vec<usize> = w.stack_at(ox, oy).filter(|m| *m < self.cogs_at.len() && !done.contains(m)).collect();
                for m in stack {
                    done.push(m);
                    self.shift(w, m, dx, dy);
                    q.push(m);
                }
            };
            let a = self.cogs_at[anchor];
            let p = self.cogs_at[c];
            let d = hypot(p.0 - a.0, p.1 - a.1);
            let r = w.cogs[anchor].radius() + w.cogs[c].radius();
            mv(w, &mut done, &mut q, c, a.0 + (p.0 - a.0) / d * r - p.0, a.1 + (p.1 - a.1) / d * r - p.1);
            let an = w.cogs[anchor].clone();
            interlock(&mut w.cogs[c], &an);
            let mut i = 0;
            while i < q.len() {
                let m = q[i];
                let pm = self.cogs_at[m];
                for &(o, how) in &self.links[m] {
                    if how != Contact::Mesh || done.contains(&o) {
                        continue;
                    }
                    let po = self.cogs_at[o];
                    let e = hypot(po.0 - pm.0, po.1 - pm.1);
                    let rm = w.cogs[m].radius() + w.cogs[o].radius();
                    let (mx, my) = (w.cogs[m].x, w.cogs[m].y);
                    mv(w, &mut done, &mut q, o, mx + (po.0 - pm.0) / e * rm - po.0, my + (po.1 - pm.1) / e * rm - po.1);
                    let mc = w.cogs[m].clone();
                    interlock(&mut w.cogs[o], &mc);
                }
                i += 1;
            }
        }
        w.solve();
    }
}

impl World {
    /// stage.js armTip: an arm's tip as drawn.
    pub fn arm_tip(&self, a: usize) -> (f64, f64) {
        let arm = &self.arms[a];
        let c = &self.cogs[arm.cog_ix];
        let ang = self.render_th(arm.cog_ix) + arm.a;
        let l = arm.r0 + arm.len;
        (c.x + ang.cos() * l, c.y + ang.sin() * l)
    }
    /// An arm's bend as drawn at the heard time: the flex angle of its last strike, for the app to ease out.
    pub fn arm_bend(&self, a: usize) -> (f64, f64) {
        (self.arms[a].bend_t, self.arms[a].bend_a)
    }
    /// stage.js beltLines: a belt's two straight runs, [x1, y1, x2, y2] each (none when the cogs overlap).
    pub fn belt_lines(&self, b: usize) -> Option<[[f64; 4]; 2]> {
        let bt = &self.belts[b];
        let (a, bb) = (&self.cogs[bt.a_ix], &self.cogs[bt.b_ix]);
        let (ra, rb) = (a.radius() + 3.0, bb.radius() + 3.0);
        let d = hypot(bb.x - a.x, bb.y - a.y);
        let base = (bb.y - a.y).atan2(bb.x - a.x);
        let cos_a = (if bt.cross { ra + rb } else { ra - rb }) / d;
        if !(cos_a.abs() < 1.0) {
            return None;
        }
        let al = cos_a.acos();
        let run = |s: f64| {
            let an = base + s * al;
            let ab = if bt.cross { an + std::f64::consts::PI } else { an };
            [a.x + ra * an.cos(), a.y + ra * an.sin(), bb.x + rb * ab.cos(), bb.y + rb * ab.sin()]
        };
        Some([run(1.0), run(-1.0)])
    }
    /// A cog's speed in rpm (signed: positive turns clockwise on screen).
    pub fn rpm(&self, c: usize) -> f64 {
        self.cogs[c].w * 60.0 / TAU
    }
}
