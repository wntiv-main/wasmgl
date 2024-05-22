#[macro_use]
mod renderer;
mod utils;

use std::collections::HashMap;
use std::iter::FromIterator;
use std::{borrow::Borrow, cmp::min, ops::Div};

use nalgebra::{Matrix4, Point3, Vector3, Vector4};
use wasm_bindgen::prelude::*;
use web_sys::{
    WebGl2RenderingContext, Window,
};

use crate::utils::set_panic_hook;
use crate::renderer::{render_loop, Shader};

#[derive(Default, Clone, Copy)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}

impl Position {
    pub fn rotate(&mut self, axis: &[f32; 3], theta: f32) {
        // Axis-angle based rotation, from https://wikimedia.org/api/rest_v1/media/math/render/svg/f259f80a746ee20d481f9b7f600031084358a27c
        let mag = (axis[0].powi(2) + axis[1].powi(2) + axis[2].powi(2)).sqrt();
        let (ux, uy, uz) = (axis[0] / mag, axis[1] / mag, axis[2] / mag);
        let (snt, cst) = theta.sin_cos();
        let ocst = 1. - cst;
        let x = (cst + ux*ux*ocst)*self.x + (ux*uy*ocst - uz*snt)*self.y + (ux*uz*ocst + uy*snt)*self.z;
        let y = (uy*ux*ocst + uz*snt)*self.x + (cst + uy*uy*ocst)*self.y + (uy*uz*ocst - ux*snt)*self.z;
        let z = (uz*ux*ocst - uy*snt)*self.x + (uz*uy*ocst + ux*snt)*self.y + (cst + uz*uz*ocst)*self.z;
        self.x = x;
        self.y = y;
        self.z = z;
    }

    pub fn average(&self, other: &Position) -> Position {
        Position {
            x: self.x + other.x / 2.,
            y: self.y + other.y / 2.,
            z: self.z + other.z / 2.,
        }
    }

    pub fn normalize(mut self) -> Self {
        let mag = (self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        self.x /= mag;
        self.y /= mag;
        self.z /= mag;
        self
    }
}

#[derive(Default, Clone, Copy)]
struct Color {
    r: f32,
    g: f32,
    b: f32,
}

#[derive(Default, Clone, Copy)]
struct Vertex {
    pos: Position,
    normal: Position,
}

#[wasm_bindgen(start)]
fn start() -> Result<(), JsValue> {
    set_panic_hook();
    let window: Window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let canvas = document.get_element_by_id("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into::<web_sys::HtmlCanvasElement>()?;

    let context = canvas
        .get_context("webgl2")?
        .unwrap()
        .dyn_into::<WebGl2RenderingContext>()?;

    context.get_extension("WEBGL_depth_texture").expect_throw("need WEBGL_depth_texture");

    context.clear_color(0., 0., 0., 1.);

    let depth_tex = context.create_texture().expect_throw("texture failed to create");
    const depth_tex_sz: usize = 512;

    context.active_texture(WebGl2RenderingContext::TEXTURE0);
    context.bind_texture(WebGl2RenderingContext::TEXTURE_2D, Some(&depth_tex));
    context.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_array_buffer_view(
        WebGl2RenderingContext::TEXTURE_2D,      // target
        0,                  // mip level
        WebGl2RenderingContext::DEPTH_COMPONENT32F as i32, // internal format
        depth_tex_sz as i32,   // width
        depth_tex_sz as i32,   // height
        0,                  // border
        WebGl2RenderingContext::DEPTH_COMPONENT, // format
        WebGl2RenderingContext::FLOAT,    // type
        None).expect_throw("error binding");              // data
    context.tex_parameteri(
        WebGl2RenderingContext::TEXTURE_2D,
        WebGl2RenderingContext::TEXTURE_MAG_FILTER,
        WebGl2RenderingContext::NEAREST as i32);
    context.tex_parameteri(
        WebGl2RenderingContext::TEXTURE_2D,
        WebGl2RenderingContext::TEXTURE_MIN_FILTER,
        WebGl2RenderingContext::NEAREST as i32);
    context.tex_parameteri(
        WebGl2RenderingContext::TEXTURE_2D,
        WebGl2RenderingContext::TEXTURE_WRAP_S,
        WebGl2RenderingContext::CLAMP_TO_EDGE as i32);
    context.tex_parameteri(
        WebGl2RenderingContext::TEXTURE_2D,
        WebGl2RenderingContext::TEXTURE_WRAP_T,
        WebGl2RenderingContext::CLAMP_TO_EDGE as i32);
    
    let depth_framebuf = context.create_framebuffer().expect_throw("creating framebuf");
    context.bind_framebuffer(WebGl2RenderingContext::FRAMEBUFFER, Some(&depth_framebuf));
    context.framebuffer_texture_2d(
        WebGl2RenderingContext::FRAMEBUFFER,       // target
        WebGl2RenderingContext::DEPTH_ATTACHMENT,  // attachment point
        WebGl2RenderingContext::TEXTURE_2D,        // texture target
        Some(&depth_tex),         // texture
        0);                   // mip level

    let attribute_locations: HashMap<&str, u32> = HashMap::from([
        ("pos", 0),
    ]);
    
    let shadow_pass = Shader::new(&context,
        include_str!("./shaders/shadow_pass.vsh"),
        include_str!("./shaders/shadow_pass.fsh"),
        &["projectionView"],
        &["pos"],
        Some(&attribute_locations));
        
    let shader = Shader::new(
        &context,
        include_str!("./shaders/main.vsh"),
        include_str!("./shaders/main.fsh"),
        &["projection", "view", "reverseLightDir", "lightPos", "shadowView"],
        &["pos", "normal"],
        Some(&attribute_locations));
    shader.enable(&context);

    let mut vao = VAO_new!(
        &context,
        (Vec::<Vertex>::new()
            // Vertex { pos: Position { x: -0.4, y: -0.4, z: -0.4 }, color: Color { r: 0.0, g: 0.0, b: 0.0 }, },
            // Vertex { pos: Position { x: 0.4, y: -0.4, z: -0.4 }, color: Color { r: 1.0, g: 0.0, b: 0.0 }, },
            // Vertex { pos: Position { x: -0.4, y: 0.4, z: -0.4 }, color: Color { r: 0.0, g: 1.0, b: 0.0 }, },
            // Vertex { pos: Position { x: -0.4, y: -0.4, z: 0.4 }, color: Color { r: 0.0, g: 0.0, b: 1.0 }, },
            // Vertex { pos: Position { x: 0.4, y: 0.4, z: -0.4 }, color: Color { r: 1.0, g: 1.0, b: 0.0 }, },
            // Vertex { pos: Position { x: -0.4, y: 0.4, z: 0.4 }, color: Color { r: 0.0, g: 1.0, b: 1.0 }, },
            // Vertex { pos: Position { x: 0.4, y: -0.4, z: 0.4 }, color: Color { r: 1.0, g: 0.0, b: 1.0 }, },
            // Vertex { pos: Position { x: 0.4, y: 0.4, z: 0.4 }, color: Color { r: 1.0, g: 1.0, b: 1.0 }, },
        , WebGl2RenderingContext::ARRAY_BUFFER, WebGl2RenderingContext::DYNAMIC_DRAW),
        (Vec::<u8>::new()
            // 0u8, 1, 2, 1, 2, 4, // Back
            // 3, 6, 5, 6, 5, 7, // Front
            // 0, 2, 3, 2, 3, 5, // Left
            // 1, 4, 6, 4, 6, 7, // Right
            // 0, 1, 3, 1, 3, 6, // Top
            // 2, 4, 5, 4, 5, 7, // Bottom
        , WebGl2RenderingContext::ELEMENT_ARRAY_BUFFER, WebGl2RenderingContext::STATIC_DRAW)
        // (vec![
        //     Position { x: 0.0, y: 0.0, z: 0.0 },
        //     Position { x: 1.0, y: 0.0, z: 0.0 },
        //     Position { x: 2.0, y: 0.0, z: 0.0 },
        //     Position { x: 0.0, y: 1.0, z: 0.0 },
        //     Position { x: 1.0, y: 1.0, z: 0.0 },
        // ], WebGl2RenderingContext::ARRAY_BUFFER, WebGl2RenderingContext::STATIC_DRAW)
    );

    let segments = 7;
    let height = 0.7;
    let mut current_height = 0.;
    let mut width = 0.03;
    let mut last_normal = Position { x: 0., y: 0., z: -1. };

    for i in 0..segments {
        let next_normal = (Position { x: 0., y: 0.1, z: -(height - current_height) * 0.3 }).normalize();
        vao.vbos.0.buffer.push(
            Vertex {
                pos: Position { x: -width, y: current_height, z: 0.1 * i as f32 },
                normal: last_normal.average(&next_normal),
            });
        vao.vbos.0.buffer.push(Vertex {
            pos: Position { x: width, y: current_height, z: 0.1 * i as f32 },
            normal: last_normal.average(&next_normal),
        });
        last_normal = next_normal;
        let len = vao.vbos.0.len() as u8;
        if i > 0 {
            vao.vbos.1.buffer.append(&mut vec![
                len - 4, len - 3, len - 2,
                len - 3, len - 2, len - 1,
            ]);
        }
        width -= width * i as f32 * 2. / segments as f32 / segments as f32;
        current_height += (height - current_height) * 0.3;
    }
    let next_normal = (Position { x: 0., y: 0.1, z: -(height - current_height) * 0.3 }).normalize();
    vao.vbos.0.buffer.push(Vertex {
        pos: Position { x: 0., y: current_height, z: 0.1 * segments as f32 },
        normal: last_normal.average(&next_normal),
    });
    let len = vao.vbos.0.len() as u8;
    vao.vbos.1.buffer.append(&mut vec![
        len - 3, len - 2, len - 1,
    ]);

    vao.vbos.0.update(&context);
    vao.vbos.1.update(&context);

    VBO_bind!(vao.vbos.0, &context, attribute_locations["pos"], Vertex, 3, WebGl2RenderingContext::FLOAT);
    VBO_bind!(vao.vbos.0, &context, shader, Vertex, normal, 3, WebGl2RenderingContext::FLOAT);
    // VBO_bind!(vao.vbos.0, &context, shader, Vertex, color, 3, WebGl2RenderingContext::FLOAT);

    // VBO_bind!(vao.vbos.2, &context, shader.find_attr("offset"), Position, 3, WebGl2RenderingContext::FLOAT);
    // context.vertex_attrib_divisor(shader.find_attr("offset"), 1);

    // let mut elements = VAO_new!(
    //     &context
    // );   

    context.enable(WebGl2RenderingContext::DEPTH_TEST);
    
    let mut proj_matrix = Matrix4::new_perspective(
        1.,
        90.0f32.to_radians(),
        0.1, 100.);
    let shadow_proj_matrix =  Matrix4::new_perspective(
        1.,
        120.0f32.to_radians(),
        0.1, 100.);
    let light_pos = Vector3::new(1., 3., -1.);
    let view_matrix = 
        Matrix4::from_euler_angles(0., 0., 0.)
            .prepend_translation(&-Vector3::new(0., 1., 0.));
    let shadow_view_matrix = 
        Matrix4::from_euler_angles(60.0f32.to_radians(), -10.0f32.to_radians(), 0.)
            .prepend_translation(&-light_pos);
    let (mut w, mut h) = (canvas.width() as i32, canvas.height() as i32);

    render_loop(move |resize: bool| {
        if resize {
            unsafe {
                canvas.set_width(
                    window
                        .inner_width()
                        .unwrap()
                        .as_f64()
                        .unwrap_or_default()
                        .to_int_unchecked::<u32>(),
                );
                canvas.set_height(
                    window
                        .inner_height()
                        .unwrap()
                        .as_f64()
                        .unwrap_or_default()
                        .to_int_unchecked::<u32>(),
                );
            }
            (w, h) = (canvas.width() as i32, canvas.height() as i32);
            proj_matrix = Matrix4::new_perspective(
                1.,
                90.0f32.to_radians(),
                0.1, 1000.);
        }
        
        for ele in &mut vao.vbos.0.buffer {
            ele.pos.rotate(&[0., 1., 0.], 1./30.);
            ele.normal.rotate(&[0., 1., 0.], 1./30.);
        }
        vao.vbos.0.update(&context);

        vao.activate(&context);
        
        shadow_pass.enable(&context);
        context.bind_framebuffer(WebGl2RenderingContext::FRAMEBUFFER, Some(&depth_framebuf));
        context.viewport(0, 0, depth_tex_sz as i32, depth_tex_sz as i32);
        context.clear(WebGl2RenderingContext::DEPTH_BUFFER_BIT);
        context.uniform_matrix4fv_with_f32_array(
            Some(shadow_pass.find_uniform("projectionView")), false,
            &(shadow_proj_matrix * shadow_view_matrix).data.as_slice());
            

        context.draw_elements_instanced_with_i32(
            WebGl2RenderingContext::TRIANGLES,
            vao.vbos.1.len() as i32,
            WebGl2RenderingContext::UNSIGNED_BYTE,
            0,
            10000
        );

        context.bind_framebuffer(WebGl2RenderingContext::FRAMEBUFFER, None);
        context.viewport(0, 0, w, h);
        context.clear(
            WebGl2RenderingContext::COLOR_BUFFER_BIT | WebGl2RenderingContext::DEPTH_BUFFER_BIT,
        );

        shader.enable(&context);

        context.uniform_matrix4fv_with_f32_array(
            Some(shader.find_uniform("projection")), false,
            (proj_matrix).data.as_slice());

        context.uniform_matrix4fv_with_f32_array(
            Some(shader.find_uniform("view")), false,
            (view_matrix).data.as_slice());

        context.uniform3fv_with_f32_array(
            Some(shader.find_uniform("lightPos")),
            (light_pos).data.as_slice());
            
        context.uniform3fv_with_f32_array(
            Some(shader.find_uniform("reverseLightDir")),
            &(shadow_view_matrix).data.as_slice()[8..11]);
            
        context.uniform_matrix4fv_with_f32_array(
            Some(shader.find_uniform("shadowView")), false,
            &(Matrix4::new_scaling(0.5).append_translation(&Vector3::new(0.5, 0.5, 0.5))
                 * shadow_proj_matrix * shadow_view_matrix)
                .data.as_slice());

        context.draw_elements_instanced_with_i32(
            WebGl2RenderingContext::TRIANGLES,
            vao.vbos.1.len() as i32,
            WebGl2RenderingContext::UNSIGNED_BYTE,
            0,
            10000
        );
    })?;

    Ok(())
}
