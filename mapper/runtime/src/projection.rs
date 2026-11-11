//! Column-major camera algebra, shared by live pointer and captured release rays.
use clbb_moba_core::Point;
#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub origin: [f64; 3],
    pub direction: [f64; 3],
}
impl Ray {
    pub fn at_plane(self, y: f64) -> Option<Point> {
        if !y.is_finite() || self.direction[1].abs() < 1e-9 {
            return None;
        }
        let t = (y - self.origin[1]) / self.direction[1];
        let x = self.origin[0] + t * self.direction[0];
        let z = self.origin[2] + t * self.direction[2];
        (t.is_finite() && t >= 0. && x.is_finite() && z.is_finite()).then(|| Point::new(x, z))
    }
}
pub fn inverse_product(projection: &[f64; 16], view: &[f64; 16]) -> Option<[f64; 16]> {
    let mut a = [[0.; 8]; 4];
    for row in 0..4 {
        for col in 0..4 {
            a[row][col] = (0..4)
                .map(|i| projection[i * 4 + row] * view[col * 4 + i])
                .sum();
        }
        a[row][row + 4] = 1.;
    }
    for col in 0..4 {
        let pivot = (col..4).max_by(|i, j| a[*i][col].abs().total_cmp(&a[*j][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        let k = a[col][col];
        for v in &mut a[col] {
            *v /= k;
        }
        let row = a[col];
        for (i, other) in a.iter_mut().enumerate() {
            if i != col {
                let k = other[col];
                for j in 0..8 {
                    other[j] -= row[j] * k;
                }
            }
        }
    }
    let inverse = std::array::from_fn(|i| a[i % 4][i / 4 + 4]);
    inverse.iter().all(|v| v.is_finite()).then_some(inverse)
}
pub fn ray(inverse: &[f64; 16], p: Point) -> Option<Ray> {
    let point = |z| {
        let v = [2. * p.x - 1., 1. - 2. * p.y, z, 1.];
        let out: [f64; 4] =
            std::array::from_fn(|i| (0..4).map(|j| inverse[j * 4 + i] * v[j]).sum());
        if out[3].abs() < 1e-12 || out.iter().any(|v| !v.is_finite()) {
            None
        } else {
            Some([out[0] / out[3], out[1] / out[3], out[2] / out[3]])
        }
    };
    let near = point(-1.)?;
    let far = point(1.)?;
    Some(Ray {
        origin: near,
        direction: std::array::from_fn(|i| far[i] - near[i]),
    })
}
#[derive(Clone,Copy,Debug,serde::Serialize)]
pub struct WallHit {pub owner:u64,pub native:u64,pub pose:[f64;12],pub point:[f64;3],pub radius:f64,pub width:f64,pub direction:[f64;2]}
/// Native arrow geometry. Offsets are world-space values read in the same
/// coherent sample as the caster and camera, never fixed pixel corrections.
#[derive(Clone,Copy,Debug,serde::Serialize)]
pub struct DirectionVisual { pub skill:i32, pub range:f64, pub start:[f64;3], pub end:[f64;3], pub observed:bool, pub target:Option<[f64;3]>, pub wall:Option<WallHit>, pub angular_cell:f64, pub surface_y:Option<f64>, pub envelope_range:f64, pub envelope_end:[f64;3] }
impl DirectionVisual {
    pub fn new(skill:i32,range:f64,start:[f64;3],end:[f64;3])->Self {Self{skill,range,start,end,observed:false,target:None,wall:None,angular_cell:0.,surface_y:None,envelope_range:range,envelope_end:end}}
    pub fn envelope(self)->Self {Self::new(self.skill,self.envelope_range,self.start,self.envelope_end)}
}

/// Only the swept contact offset changes the physical origin. No wall target
/// projection, reach clamp or selected-point lock is applied to cursor intent.
#[derive(Default)]
pub struct WallSurface {epoch:String,key:Option<(u64,u64)>,pose:Option<[f64;12]>,a:Option<[f64;3]>,b:Option<[f64;3]>}
impl WallSurface {
    pub fn offset(&mut self,epoch:&str,hero:[f64;3],hit:Option<WallHit>)->Point {
        if self.epoch!=epoch {*self=Self::default();self.epoch=epoch.into();}
        let Some(h)=hit else{return Point::default();};
        if self.key!=Some((h.owner,h.native))||self.pose!=Some(h.pose) {
            self.key=Some((h.owner,h.native));self.pose=Some(h.pose);self.a=Some(h.point);self.b=None;return Point::default();
        }
        let a=self.a.unwrap();let last=self.b.unwrap_or(a);
        let distance=|p:[f64;3],q:[f64;3]|(0..3).map(|i|(p[i]-q[i]).powi(2)).sum::<f64>().sqrt();
        if distance(last,h.point)>=0.05 {
            if let Some(b)=self.b {
                let d:[f64;3]=std::array::from_fn(|i|b[i]-a[i]);let n=distance(a,b);
                let v:[f64;3]=std::array::from_fn(|i|h.point[i]-a[i]);let t=(0..3).map(|i|v[i]*d[i]).sum::<f64>()/(n*n);
                if (0..3).map(|i|(v[i]-t*d[i]).powi(2)).sum::<f64>().sqrt()>0.01 {self.a=Some(b);}
            }
            self.b=Some(h.point);
        }
        let (Some(a),Some(b))=(self.a,self.b)else{return Point::default();};
        let tx=b[0]-a[0];let tz=b[2]-a[2];let n=tx.hypot(tz);if n<1e-6{return Point::default();}
        let mut normal=Point::new(tz/n,-tx/n);
        if normal.x*(hero[0]-h.point[0])+normal.y*(hero[2]-h.point[2])<0.{normal=normal*-1.;}
        let offset=normal*h.width;let query=Point::new(h.point[0]+offset.x-hero[0],h.point[2]+offset.y-hero[2]);let length=query.x.hypot(query.y);
        if length<1e-6||(query.x*h.direction[0]+query.y*h.direction[1])/length<0.9999619230641713{return Point::default();}
        offset
    }
}

/// Unproject the actual cursor POINT onto the native marker plane. Native
/// contact distance never fixes the input radius or locks the finger angle.
pub fn direction_input(view:&[f64;16],projection:&[f64;16],hero:[f64;3],pointer:Point,spec:DirectionVisual,offset:Point)->Option<Point> {
    if let Some(y)=spec.surface_y {
        let target=ray(&inverse_product(projection,view)?,pointer)?.at_plane(y)?;
        let direction=Point::new(target.x-spec.envelope_end[0]+offset.x-hero[0],target.y-spec.envelope_end[2]+offset.y-hero[2]);let n=direction.x.hypot(direction.y);
        return (n>1e-8&&n.is_finite()).then(||direction/n);
    }
    visual_direction(view,projection,hero,pointer,spec.envelope())
}

/// Intersect the cursor's screen ray from the projected arrow origin with the
/// projected native endpoint circle. The 3D circle may have a different height
/// from the caster; projecting onto a flat caster plane cannot represent it.
pub fn visual_direction(view:&[f64;16],projection:&[f64;16],hero:[f64;3],pointer:Point,spec:DirectionVisual)->Option<Point> {
    if (!pointer.x.is_finite() || !pointer.y.is_finite()) || !spec.range.is_finite() || spec.range<=0. || spec.range>1e4
        || hero.iter().chain(spec.start.iter()).chain(spec.end.iter()).any(|v|!v.is_finite()||v.abs()>1e6){return None;}
    let clip=|p:[f64;4]|crate::math::matvec(projection,crate::math::matvec(view,p));
    let start=clip([hero[0]+spec.start[0],hero[1]+spec.start[1],hero[2]+spec.start[2],1.]);
    if start[3]<=1e-6{return None;}
    let sx=start[0]/start[3];let sy=start[1]/start[3];let px=2.*pointer.x-1.;let py=1.-2.*pointer.y;
    let dx=px-sx;let dy=py-sy;if dx.hypot(dy)<1e-8{return None;}
    let line=[sy-py,px-sx,sx*py-sy*px];
    let dot=|p:[f64;4]|line[0]*p[0]+line[1]*p[1]+line[2]*p[3];
    let a=dot(clip([spec.range,0.,0.,0.]));let b=dot(clip([0.,0.,spec.range,0.]));
    let c=dot(clip([hero[0]+spec.end[0],hero[1]+spec.end[1],hero[2]+spec.end[2],1.]));
    let norm=a.hypot(b);if !norm.is_finite()||norm<1e-10||c.abs()>norm+1e-8{return None;}
    let angle=b.atan2(a);let delta=(-c/norm).clamp(-1.,1.).acos();let mut best=None;let mut nearest=f64::INFINITY;
    for theta in [angle+delta,angle-delta] {
        let dir=Point::new(theta.cos(),theta.sin());
        let p=clip([hero[0]+spec.end[0]+dir.x*spec.range,hero[1]+spec.end[1],hero[2]+spec.end[2]+dir.y*spec.range,1.]);
        if p[3]<=1e-6{continue;}
        let ex=p[0]/p[3]-sx;let ey=p[1]/p[3]-sy;
        if ex*dx+ey*dy<=0.{continue;}
        let distance=(ex-dx).hypot(ey-dy);if distance<nearest {nearest=distance;best=Some(dir);}
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_recorded_marker_center_reconstructs_native_heading() {
        let frames:serde_json::Value=serde_json::from_str(include_str!("../test-data/ling-marker-center.json")).unwrap();
        let mut worst=1f64;let mut old_bad=0;
        for frame in frames.as_array().unwrap() {
            let view=serde_json::from_value(frame["view"].clone()).unwrap();
            let projection=serde_json::from_value(frame["projection"].clone()).unwrap();
            let hero=serde_json::from_value(frame["hero_render"].clone()).unwrap();
            let point:[f64;3]=serde_json::from_value(frame["point"].clone()).unwrap();
            let prior=serde_json::from_value(frame["prior"].clone()).unwrap();
            let native:[f64;2]=serde_json::from_value(frame["direction"].clone()).unwrap();
            let width=frame["width"].as_f64().unwrap();
            let hit=WallHit{owner:1,native:2,pose:[0.;12],point:prior,radius:1.3,width,direction:native};
            let mut surface=WallSurface::default();surface.offset("scene",hero,Some(hit));
            let hit=WallHit{point,..hit};let offset=surface.offset("scene",hero,Some(hit));
            assert!((offset.x.hypot(offset.y)-width).abs()<1e-8);
            let p=crate::math::project(&view,&projection,point,false).unwrap();
            let mut spec=DirectionVisual::new(8410,12.,[0.;3],[0.,1.5,0.]);spec.surface_y=Some(point[1]);
            let direction=direction_input(&view,&projection,hero,Point::new(p[0],p[1]),spec,offset).unwrap();
            let old=visual_direction(&view,&projection,hero,Point::new(p[0],p[1]),spec.envelope()).unwrap();
            if old.x*native[0]+old.y*native[1]<0.9999619230641713{old_bad+=1;}
            let cosine=direction.x*native[0]+direction.y*native[1];worst=worst.min(cosine);
            assert!(cosine>0.9999619230641713,"Actual marker center must reconstruct native direction");
            // An arbitrary new cursor must remain free; no reach/face clamp.
            let a=direction_input(&view,&projection,hero,Point::new(0.1,0.2),spec,offset).unwrap();
            let b=direction_input(&view,&projection,hero,Point::new(0.1,0.8),spec,offset).unwrap();
            assert!(a.x*b.x+a.y*b.y<0.99);
            // Another wall/scene cannot retain the old origin correction.
            assert_eq!(surface.offset("other",hero,Some(hit)),Point::default());
        }
        assert!(frames.as_array().unwrap().len()>=16);assert!(worst>0.9999619230641713);assert!(old_bad>=12,"Fixture must expose the old center error");
    }
    #[test]
    fn recorded_wall_snap_never_freezes_cursor_direction() {
        let clips:serde_json::Value=serde_json::from_str(include_str!("../test-data/ling-wall-frozen-stick.json")).unwrap();
        for clip in clips.as_array().unwrap() {
            let mut directions=Vec::new();let mut commands=Vec::new();
            for sample in clip["samples"].as_array().unwrap() {
                let view=serde_json::from_value(sample["view"].clone()).unwrap();
                let projection=serde_json::from_value(sample["projection"].clone()).unwrap();
                let hero=serde_json::from_value(sample["hero_render"].clone()).unwrap();
                let pointer:[f64;2]=serde_json::from_value(sample["pointer"].clone()).unwrap();
                let v=&sample["spec"];
                let mut spec=DirectionVisual::new(8410,v["envelope_range"].as_f64().unwrap(),serde_json::from_value(v["start"].clone()).unwrap(),serde_json::from_value(v["envelope_end"].clone()).unwrap());
                let cursor=Point::new(pointer[0],pointer[1]);
                let dir=direction_input(&view,&projection,hero,cursor,spec,Point::default()).unwrap();
                directions.push(dir);
                let center:[f64;2]=serde_json::from_value(sample["center"].clone()).unwrap();
                let radius:[f64;2]=serde_json::from_value(sample["radius"].clone()).unwrap();
                let resolution:[f64;2]=serde_json::from_value(sample["resolution"].clone()).unwrap();
                let mut solver=clbb_moba_core::AngularRadialSolver::default();
                assert!(solver.configure(Point::new(center[0]*resolution[0],center[1]*resolution[1]),(radius[0]*resolution[0]+radius[1]*resolution[1])*0.5,[view[0],-view[1],view[8],-view[9]],1.,Point::new(resolution[0],resolution[1])));
                let command=solver.predict(dir).unwrap();
                commands.push((command.x.round()as i64,command.y.round()as i64));

                // Native snap/shortening must never steer or clamp the finger.
                spec.range=0.5;spec.end=[0.,-3.,0.];spec.observed=true;spec.target=Some([0.;3]);
                let changed=direction_input(&view,&projection,hero,cursor,spec,Point::default()).unwrap();
                assert!((dir.x-changed.x).hypot(dir.y-changed.y)<1e-12);
            }
            assert_ne!(commands[0],commands[1],"Recorded sweep must change quantized virtual finger");
            let a=directions[0];let b=directions[1];
            assert!(a.x*b.x+a.y*b.y<1.-1e-6,"Recorded mouse sweep must move the stick: {}",clip["clip"]);
        }
    }
    #[test]
    fn shortened_wall_endpoint_at_another_height_uses_live_circle() {
        let view=[0.7077527046203613,0.48100554943084717,-0.5174165964126587,0.,-0.0008955360390245914,0.7330173254013062,0.6802094578742981,0.,-0.7064598202705383,0.4809567332267761,-0.5192257165908813,0.,24.3,12.7,-35.9,1.];
        let projection=[2.0574307441711426,0.,0.,0.,0.,2.645362615585327,0.,0.,0.,0.,-1.000800371170044,-1.,0.,0.,-0.4001600742340088,0.];
        for height in [0.55,0.87,1.51] {for range in [2.,4.5,8.,12.] {for angle in [-1_f64,0.3,2.] {
            let hero=[-29.,height,7.];let tip=[hero[0]+range*angle.cos(),1.5,hero[2]+range*angle.sin()];
            let p=crate::math::project(&view,&projection,tip,false).unwrap();
            let spec=DirectionVisual::new(8410,range,[0.;3],[0.,1.5-height,0.]);
            let dir=visual_direction(&view,&projection,hero,Point::new(p[0],p[1]),spec).unwrap();
            let actual=crate::math::project(&view,&projection,[hero[0]+range*dir.x,1.5,hero[2]+range*dir.y],false).unwrap();
            assert!((actual[0]-p[0]).hypot(actual[1]-p[1])<1e-9);
        }}}
    }
    #[test]
    fn raised_arc_tracks_screen_cursor_across_wall_heights_and_panorama() {
        let base_view=[0.7077527046203613,0.48100554943084717,-0.5174165964126587,0.0,-0.0008955360390245914,0.7330173254013062,0.6802094578742981,0.0,-0.7064598202705383,0.4809567332267761,-0.5192257165908813,0.0,19.310880661010742,7.635412216186523,-28.36872673034668,1.0];
        let projection=[2.0574307441711426,0.0,0.0,0.0,0.0,2.645362615585327,0.0,0.0,0.0,0.0,-1.000800371170044,-1.0,0.0,0.0,-0.4001600742340088,0.0];
        let mut old_error:f64=0.;
        for height in [0.55,0.87,1.51] {
            let hero=[-22.96875,height,5.59765625];
            for pan in [[0.,0.],[1.4,-0.8],[-2.,1.2]] {
                let mut view=base_view;
                for row in 0..3 {view[12+row]-=base_view[row]*pan[0]+base_view[8+row]*pan[1];}
                let spec=DirectionVisual::new(8410,12.,[0.,0.,0.],[0.,1.5,0.]);
                for pointer in [Point::new(0.1,0.5),Point::new(0.9,0.5),Point::new(0.5,0.95),Point::new(0.9,0.9)] {
                    let dir=visual_direction(&view,&projection,hero,pointer,spec).unwrap();
                    assert!((dir.x.hypot(dir.y)-1.).abs()<1e-10);
                    let start=crate::math::project(&view,&projection,hero,false).unwrap();
                    let endpoint=|dir:Point|crate::math::project(&view,&projection,[hero[0]+12.*dir.x,hero[1]+1.5,hero[2]+12.*dir.y],false).unwrap();
                    let e=endpoint(dir);let dx=pointer.x-start[0];let dy=pointer.y-start[1];
                    assert!(((e[0]-start[0])*dy-(e[1]-start[1])*dx).abs()<1e-10);
                    assert!((e[0]-start[0])*dx+(e[1]-start[1])*dy>0.);
                    let inv=inverse_product(&projection,&view).unwrap();let target=ray(&inv,pointer).unwrap().at_plane(height).unwrap();
                    let x=target.x-hero[0];let z=target.y-hero[2];let size=x.hypot(z);
                    let old=endpoint(Point::new(x/size,z/size));
                    old_error=old_error.max(((old[0]-start[0])*dy-(old[1]-start[1])*dx).abs()/dx.hypot(dy)*884.);
                }
            }
        }
        assert!(old_error>20.,"Fixture must reproduce the visible flat-plane error");
    }
    #[test]
    fn visual_projection_rejects_invalid_native_geometry() {
        let matrix=[0.;16];let spec=DirectionVisual::new(8410,12.,[0.;3],[0.,1.5,0.]);
        assert!(visual_direction(&matrix,&matrix,[0.;3],Point::new(0.5,0.5),spec).is_none());
        assert!(visual_direction(&matrix,&matrix,[0.;3],Point::new(f64::NAN,0.5),spec).is_none());
    }
    #[test]
    fn captured_ray_resolves_new_height_without_new_camera() {
        let r = Ray {
            origin: [2., 10., 3.],
            direction: [1., -2., 4.],
        };
        assert_eq!(r.at_plane(0.), Some(Point::new(7., 23.)));
        assert_eq!(r.at_plane(4.), Some(Point::new(5., 15.)));
        assert!(r.at_plane(12.).is_none());
    }
    #[test]
    fn inverse_handles_axis_rotation_and_translation() {
        let v = [
            1., 0., 0., 0., 0., 0., 1., 0., 0., -1., 0., 0., -2., 3., -4., 1.,
        ];
        let p = [
            2., 0., 0., 0., 0., 3., 0., 0., 0., 0., 4., 0., 0., 0., 0., 1.,
        ];
        let inv = inverse_product(&p, &v).unwrap();
        let target = [1., 2., 3., 1.];
        let mv = |m: &[f64; 16], x: [f64; 4]| -> [f64; 4] {
            std::array::from_fn(|i| (0..4).map(|j| m[j * 4 + i] * x[j]).sum())
        };
        let result = mv(&inv, mv(&p, mv(&v, target)));
        for i in 0..4 {
            assert!((result[i] - target[i]).abs() < 1e-10);
        }
    }
    #[test]
    fn panorama_camera_translation_keeps_bottom_and_corner_cursor_projection_exact() {
        // Actual camera from the failing held-Q bottom sweep at4000x3111.
        let view = [0.7077527046203613,0.48100554943084717,-0.5174165964126587,0.,
            -0.0008955360390245914,0.7330173254013062,0.6802094578742981,0.,
            -0.7064598202705383,0.4809567332267761,-0.5192257165908813,0.,
            35.274085998535156,24.113412857055664,-42.08110427856445,1.];
        let projection = [2.0574307441711426,0.,0.,0.,0.,2.645362615585327,0.,0.,
            0.,0.,-1.000800371170044,-1.,0.,0.,-0.4001600742340088,0.];
        let inverse = inverse_product(&projection, &view).unwrap();
        let shift = [1.4, -0.8];
        let mut moved = view;
        for row in 0..3 {
            moved[12 + row] -= view[row] * shift[0] + view[8 + row] * shift[1];
        }
        let moved_inverse = inverse_product(&projection, &moved).unwrap();
        for pointer in [Point::new(0.5,0.95),Point::new(0.5,1.),
                        Point::new(0.,0.),Point::new(1.,0.),Point::new(0.,1.),Point::new(1.,1.)] {
            let original_ray = ray(&inverse,pointer).unwrap();
            let world = original_ray.at_plane(0.1).unwrap();
            let world_moved = ray(&moved_inverse,pointer).unwrap().at_plane(0.1).unwrap();
            assert!((world_moved.x-world.x-shift[0]).abs()<1e-9);
            assert!((world_moved.y-world.y-shift[1]).abs()<1e-9);
            let projected = crate::math::project(&moved,&projection,[world_moved.x,0.1,world_moved.y],false).unwrap();
            assert!((projected[0]-pointer.x).abs()<1e-10);
            assert!((projected[1]-pointer.y).abs()<1e-10);
            // A release keeps its captured ray even after the camera moves.
            assert_eq!(original_ray.at_plane(0.1).unwrap(),world);
        }
    }
}
