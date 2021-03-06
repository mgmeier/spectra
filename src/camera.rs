use serde::Deserialize;
use serde_json::from_reader;
use std::default::Default;
use std::fs::File;
use std::path::Path;

use linear::{Axis, Matrix4, Orientation, Position, Quaternion, Rotate, ToHomogeneous, Translation,
             Unit, UnitQuaternion, Vector3, X_AXIS, Y_AXIS, Z_AXIS, translation_matrix};
use projection::{Perspective, Projectable};
use resource::{Load, LoadError, ResCache};
use transform::Transformable;

#[derive(Clone, Debug)]
pub struct Camera<P> {
  pub position: Position,
  pub orientation: Orientation,
  pub properties: P
}

impl<P> Camera<P> {
  pub fn new(position: Position, orientation: Orientation, properties: P) -> Self {
    Camera {
      position: position,
      orientation: orientation,
      properties: properties
    }
  }
}

impl<P> Default for Camera<P> where P: Default {
  fn default() -> Self {
    Camera::new(Position::new(0., 0., 0.),
                Orientation::from_unit_value_unchecked(Quaternion::from_parts(1., Vector3::new(0., 0., 0.))),
                P::default())
  }
}

impl<T> Projectable for Camera<T> where T: Projectable {
  fn projection(&self) -> Matrix4<f32> {
    self.properties.projection()
  }
}

impl<P> Transformable for Camera<P> {
  fn transform(&self) -> Matrix4<f32> {
    self.orientation.to_rotation_matrix().to_homogeneous() * translation_matrix(-self.position)
  }
}

#[derive(Deserialize)]
struct Manifest<P> {
  position: [f32; 3],
  orientation: [f32; 4],
  #[serde(default)]
  properties: P
}

impl<A> Load for Camera<A> where A: Default + Deserialize {
  type Args = ();

  const TY_STR: &'static str = "cameras";

  fn load<P>(path: P, _: &mut ResCache, _: Self::Args) -> Result<Self, LoadError> where P: AsRef<Path> {
    let path = path.as_ref();

    info!("loading camera {:?}", path);

    let manifest: Manifest<A> = {
      let file = File::open(path).map_err(|e| LoadError::FileNotFound(path.to_path_buf(), format!("{:?}", e)))?;
      from_reader(file).map_err(|e| LoadError::ParseFailed(format!("{:?}", e)))?
    };

    Ok(Camera {
      position: (&manifest.position).into(),
      orientation: Unit::new(&Quaternion::from(&manifest.orientation)),
      properties: manifest.properties
    })
  }
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Freefly {
  // sensitivities
  #[serde(default = "def_yaw_sens")]
  pub yaw_sens: f32,
  #[serde(default = "def_pitch_sens")]
  pub pitch_sens: f32,
  #[serde(default = "def_roll_sens")]
  pub roll_sens: f32,
  #[serde(default = "def_forward_sens")]
  pub forward_sens: f32,
  #[serde(default = "def_strafe_sens")]
  pub strafe_sens: f32,
  #[serde(default = "def_upward_sens")]
  pub upward_sens: f32,
  // projection
  pub perspective: Perspective
}

impl Freefly {
  pub fn new() -> Self {
    Freefly {
      yaw_sens: def_yaw_sens(),
      pitch_sens: def_pitch_sens(),
      roll_sens: def_roll_sens(),
      forward_sens: def_forward_sens(),
      strafe_sens: def_strafe_sens(),
      upward_sens: def_upward_sens(),
      perspective: Perspective::default(),
    }
  }
}

impl Default for Freefly {
  fn default() -> Self {
    Self::new()
  }
}

impl Projectable for Freefly {
  fn projection(&self) -> Matrix4<f32> {
    self.perspective.projection()
  }
}

impl Camera<Freefly> {
  pub fn mv(&mut self, dir: Translation) {
    let p = &self.properties;
    let axis = dir * Axis::new(p.strafe_sens, p.upward_sens, p.forward_sens);
    let v = self.orientation.inverse_rotate(&axis);

    self.position -= v;
  }

  pub fn look_around(&mut self, dir: Translation) {
    let p = &self.properties;

    fn orient(axis: &Axis, phi: f32) -> Orientation {
      UnitQuaternion::from_axisangle(Unit::new(&axis), phi)
    }

    self.orientation = orient(&Y_AXIS, p.yaw_sens * dir.y) * self.orientation;
    self.orientation = orient(&X_AXIS, p.pitch_sens * dir.x) * self.orientation;
    self.orientation = orient(&Z_AXIS, p.roll_sens * dir.z) * self.orientation;
  }
}

fn def_yaw_sens() -> f32 { 0.01 }
fn def_pitch_sens() -> f32 { 0.01 }
fn def_roll_sens() -> f32 { 0.01 }
fn def_forward_sens() -> f32 { 0.1 }
fn def_strafe_sens() -> f32 { 0.1 }
fn def_upward_sens() -> f32 { 0.1 }
