// crt-royale-fast pass 3: crt-royale-mask-resize-horizontal.slang
// crt-royale (C) 2014 TroggleMonkey, crt-royale-fast by Hyllian (2024), GPL-2.0-or-later.
// From libretro/slang-shaders crt/shaders/crt-royale/src-fast, converted to GLSL ES 3.00
// by tools/royale/convert.py (parameters fixed at their preset defaults).


uniform vec4 SourceSize;
uniform vec4 OriginalSize;
uniform vec4 OutputSize;
uniform float FrameCount;

#ifndef BIND_SHADER_PARAMS_H
#define BIND_SHADER_PARAMS_H

#ifndef USER_SETTINGS_H
#define USER_SETTINGS_H

#define RUNTIME_SHADER_PARAMS_ENABLE
#define RUNTIME_PHOSPHOR_BLOOM_SIGMA
#define RUNTIME_ANTIALIAS_WEIGHTS

#define RUNTIME_SCANLINES_HORIZ_FILTER_COLORSPACE
#define RUNTIME_GEOMETRY_TILT
#define RUNTIME_GEOMETRY_MODE
#define FORCE_RUNTIME_PHOSPHOR_MASK_MODE_TYPE_SELECT
    #define PHOSPHOR_MASK_MANUALLY_RESIZE
    #define PHOSPHOR_MASK_RESIZE_LANCZOS_WINDOW
    #define PHOSPHOR_BLOOM_TRIADS_LARGER_THAN_3_PIXELS

#define crt_gamma_static (float(2.5))
#define lcd_gamma_static (float(2.2))

#define levels_contrast_static (float(1.0))
#define levels_autodim_temp (float(0.5))

#define halation_weight_static (float(0.0))
#define diffusion_weight_static (float(0.075))
#define bloom_underestimate_levels_static (float(0.8))
#define bloom_excess_static (float(0.0))

#define bloom_approx_filter_static (float(0.0))

#define beam_num_scanlines (float(3.0))
#define beam_generalized_gaussian (bool(true))
#define beam_antialias_level (float(1.0))
#define beam_min_sigma_static (float(0.02))
#define beam_max_sigma_static (float(0.3))
#define beam_spot_shape_function (float(0.0))
#define beam_spot_power_static (float(1.0/3.0))
#define beam_min_shape_static (float(2.0))
#define beam_max_shape_static (float(4.0))
#define beam_shape_power_static (float(1.0/4.0))
#define beam_horiz_filter_static (float(0.0))
#define beam_horiz_sigma_static (float(0.35))
#define beam_horiz_linear_rgb_weight_static (float(1.0))
#define beam_misconvergence (bool(true))
#define convergence_offsets_r_static (vec2(vec2 (0.1, 0.2)))
#define convergence_offsets_g_static (vec2(vec2 (0.3, 0.4)))
#define convergence_offsets_b_static (vec2(vec2 (0.5, 0.6)))
#define interlace_detect_static (bool(true))
#define interlace_1080i_static (bool(false))
#define interlace_bff_static (bool(false))

#define aa_level (float(12.0))
#define aa_filter (float(6.0))
#define aa_temporal (bool(false))
#define aa_subpixel_r_offset_static (vec2(vec2 (-1.0/3.0, 0.0)))
#define aa_cubic_c_static (float(0.5))
#define aa_gauss_sigma_static (float(0.5))

#define mask_type_static (float(1.0))
#define mask_sample_mode_static (float(0.0))
#define mask_specify_num_triads_static (float(0.0))
#define mask_triad_size_desired_static (float(24.0 / 8.0))
#define mask_num_triads_desired_static (float(480.0))
#define mask_sinc_lobes (float(3.0))
#define mask_min_allowed_triad_size (float(2.0))

#define geom_mode_static (float(0.0))
#define geom_radius_static (float(2.0))
#define geom_view_dist_static (float(2.0))
#define geom_tilt_angle_static (vec2(vec2 (0.0, 0.0)))
#define geom_aspect_ratio_static (float(1.313069909))
#define geom_overscan_static (vec2(vec2 (1.0, 1.0)))
#define geom_force_correct_tangent_matrix (bool(true))

#define border_size_static (float(0.015))
#define border_darkness_static (float(2.0))
#define border_compress_static (float(2.5))

#endif

#ifndef DERIVED_SETTINGS_AND_CONSTANTS_H
#define DERIVED_SETTINGS_AND_CONSTANTS_H

#ifndef USER_CGP_CONSTANTS_H
#define USER_CGP_CONSTANTS_H

#define bloom_approx_size_x (float(320.0))
#define bloom_approx_size_x_for_fake (float(400.0))
#define mask_resize_viewport_scale (vec2(vec2(0.0625, 0.0625)))
#define geom_max_aspect_ratio (float(4.0/3.0))

#define mask_texture_small_size (vec2(vec2(64.0, 64.0)))

#define mask_triads_per_tile (float(8.0))

#define mask_grille15_avg_color (float(53.0/255.0))
#define mask_slot_avg_color (float(46.0/255.0))
#define mask_shadow_avg_color (float(41.0/255.0))

#define mask_grille_avg_color (float(mask_grille15_avg_color))

#endif

#define FIX_ZERO(c) (max(abs(c), 0.0000152587890625))

#define bloom_approx_filter (float(bloom_approx_filter_static))
#define mask_resize_src_lut_size (vec2(mask_texture_small_size))

    #ifdef FORCE_RUNTIME_PHOSPHOR_MASK_MODE_TYPE_SELECT
        #define RUNTIME_PHOSPHOR_MASK_MODE_TYPE_SELECT
    #endif

#define max_aa_base_pixel_border (float(0.0))

#define max_aniso_pixel_border (float(max_aa_base_pixel_border + 0.5))

#define max_tiled_pixel_border (float(max_aniso_pixel_border))

#define max_mask_texel_border (float(ceil(max_tiled_pixel_border)))

#define max_mask_tile_border (float(max_mask_texel_border/ (mask_min_allowed_triad_size * mask_triads_per_tile)))

#define mask_resize_num_tiles (float(1.0 + 2.0 * max_mask_tile_border))
#define mask_start_texels (float(max_mask_texel_border))

#define mask_resize_num_triads (float(mask_resize_num_tiles * mask_triads_per_tile))
#define min_allowed_viewport_triads (vec2(vec2(mask_resize_num_triads) / mask_resize_viewport_scale))

#define pi (float(3.141592653589))
#define under_half (float(0.4995))

#endif

#define OVERRIDE_DEVICE_GAMMA
#define gba_gamma (float(3.5))
#define ANTIALIAS_OVERRIDE_BASICS
#define ANTIALIAS_OVERRIDE_PARAMETERS

#define crt_gamma (2.4)

#define lcd_gamma (2.4)

#define levels_contrast (0.671875)

#define bloom_underestimate_levels (1.0)

#define beam_min_sigma (0.055)

#define beam_max_sigma (0.2)

#define beam_spot_power (0.38)

#define beam_min_shape (2.0)

#define beam_max_shape (2.0)

#define beam_shape_power (0.25)

#define beam_horiz_filter (0.0)

#define beam_horiz_sigma (0.35)

#define convergence_offset_x_r (0.0)

#define convergence_offset_x_g (0.0)

#define convergence_offset_x_b (0.0)

#define convergence_offset_y_r (0.05)

#define convergence_offset_y_g (-0.05)

#define convergence_offset_y_b (0.05)

#define mask_type (0.0)

#define aa_subpixel_r_offset_x_runtime (-0.333333333)

#define aa_subpixel_r_offset_y_runtime (0.0)

#define aa_cubic_c (0.5)

#define aa_gauss_sigma (0.5)

#define interlace_detect (bool(bool((1.0))))

#define interlace_1080i (0.0)

vec2 get_aspect_vector(const float geom_aspect_ratio)
{

    float geom_clamped_aspect_ratio =
        min(geom_aspect_ratio, geom_max_aspect_ratio);
    vec2 geom_aspect =
        normalize(vec2(geom_clamped_aspect_ratio, 1.0));
    return geom_aspect;
}

vec3 get_convergence_offsets_x_vector()
{
    return vec3(convergence_offset_x_r, convergence_offset_x_g,
        convergence_offset_x_b);
}

vec3 get_convergence_offsets_y_vector()
{
    return vec3(convergence_offset_y_r, convergence_offset_y_g,
        convergence_offset_y_b);
}

vec2 get_convergence_offsets_r_vector()
{
    return vec2(convergence_offset_x_r, convergence_offset_y_r);
}

vec2 get_convergence_offsets_g_vector()
{
    return vec2(convergence_offset_x_g, convergence_offset_y_g);
}

vec2 get_convergence_offsets_b_vector()
{
    return vec2(convergence_offset_x_b, convergence_offset_y_b);
}

vec2 get_aa_subpixel_r_offset()
{
    return aa_subpixel_r_offset_static;
}

float get_mask_amplify()
{
    float mask_grille_amplify = 1.0/mask_grille_avg_color;
    float mask_slot_amplify = 1.0/mask_slot_avg_color;
    float mask_shadow_amplify = 1.0/mask_shadow_avg_color;
    return mask_type < 0.5 ? mask_grille_amplify :
        mask_type < 1.5 ? mask_slot_amplify :
        mask_shadow_amplify;
}

#endif

#ifndef PHOSPHOR_MASK_RESIZING_H
#define PHOSPHOR_MASK_RESIZING_H

        #define USE_SINGLE_STATIC_LOOP

#define mask_min_allowed_tile_size (float(ceil(mask_min_allowed_triad_size * mask_triads_per_tile)))
#define mask_min_expected_tile_size (float(mask_min_allowed_tile_size))

#define pi_over_lobes (float(pi/mask_sinc_lobes))
#define max_sinc_resize_samples_float (float(2.0 * mask_sinc_lobes * mask_resize_src_lut_size.x/mask_min_expected_tile_size))

#define max_sinc_resize_samples_m4 (float(ceil(max_sinc_resize_samples_float * 0.25) * 4.0))

float get_dynamic_loop_size(float magnification_scale)
{

    float min_samples_float = 2.0 * mask_sinc_lobes / magnification_scale;
    float min_samples_m4 = ceil(min_samples_float * 0.25) * 4.0;

    float max_samples_m4 = min(128.0, max_sinc_resize_samples_m4);

    return min(min_samples_m4, max_samples_m4);
}

vec2 get_first_texel_tile_uv_and_dist(vec2 tex_uv, vec2 tex_size, float dr, float input_tiles_per_texture_r, float samples, bool vertical)
{

    vec2 curr_texel  = tex_uv * tex_size;
    vec2 prev_texel  = floor(curr_texel - vec2(under_half)) + vec2(0.5);
    vec2 first_texel = prev_texel - vec2(samples/2.0 - 1.0);
    vec2 first_texel_uv_wrap_2D = first_texel * dr;
    vec2 first_texel_dist_2D    = curr_texel - first_texel;

    vec2 first_texel_tile_uv_wrap_2D = first_texel_uv_wrap_2D * input_tiles_per_texture_r;

    vec2 coord_negative = vec2((first_texel_tile_uv_wrap_2D.x < 0.),(first_texel_tile_uv_wrap_2D.y < 0.));
    vec2 first_texel_tile_uv_2D = fract(first_texel_tile_uv_wrap_2D) + coord_negative;

    vec2 tile_u_and_dist = vec2(first_texel_tile_uv_2D.x, first_texel_dist_2D.x);
    vec2 tile_v_and_dist = vec2(first_texel_tile_uv_2D.y, first_texel_dist_2D.y);

    return vertical ? tile_v_and_dist : tile_u_and_dist;
}

vec4 tex2Dlod0try(sampler2D tex, vec2 tex_uv)
{

    return texture(tex, tex_uv);
}

    #define CALCULATE_R_COORD_FOR_4_SAMPLES                                             vec4 true_i = vec4(i_base + i) + vec4(0.0, 1.0, 2.0, 3.0);          vec4 tile_uv_r = fract(                                                      first_texel_tile_uv_rrrr + true_i * tile_dr);                               vec4 tex_uv_r = tile_uv_r * tile_size_uv_r;

    #ifdef PHOSPHOR_MASK_RESIZE_LANCZOS_WINDOW
        #define CALCULATE_SINC_RESAMPLE_WEIGHTS                                             vec4 pi_dist_over_lobes = pi_over_lobes * dist;                         vec4 weights = min(sin(pi_dist) * sin(pi_dist_over_lobes) /                 (pi_dist*pi_dist_over_lobes), vec4(1.0));
    #else
        #define CALCULATE_SINC_RESAMPLE_WEIGHTS                                             vec4 weights = min(sin(pi_dist)/pi_dist, vec4(1.0));
    #endif

    #define UPDATE_COLOR_AND_WEIGHT_SUMS                                                vec4 dist = magnification_scale *                                           abs(first_dist_unscaled - true_i);                                          vec4 pi_dist = pi * dist;                                               CALCULATE_SINC_RESAMPLE_WEIGHTS;                                                pixel_color += new_sample0 * weights.xxx;                                       pixel_color += new_sample1 * weights.yyy;                                       pixel_color += new_sample2 * weights.zzz;                                       pixel_color += new_sample3 * weights.www;                                       weight_sum += weights;

    #define VERTICAL_SINC_RESAMPLE_LOOP_BODY                                            CALCULATE_R_COORD_FOR_4_SAMPLES;                                                vec3 new_sample0 = tex2Dlod0try(tex,                                    vec2(tex_uv.x, tex_uv_r.x)).rgb;                                          vec3 new_sample1 = tex2Dlod0try(tex,                                    vec2(tex_uv.x, tex_uv_r.y)).rgb;                                          vec3 new_sample2 = tex2Dlod0try(tex,                                    vec2(tex_uv.x, tex_uv_r.z)).rgb;                                          vec3 new_sample3 = tex2Dlod0try(tex,                                    vec2(tex_uv.x, tex_uv_r.w)).rgb;                                          UPDATE_COLOR_AND_WEIGHT_SUMS;

    #define HORIZONTAL_SINC_RESAMPLE_LOOP_BODY                                          CALCULATE_R_COORD_FOR_4_SAMPLES;                                                vec3 new_sample0 = tex2Dlod0try(tex,                                    vec2(tex_uv_r.x, tex_uv.y)).rgb;                                          vec3 new_sample1 = tex2Dlod0try(tex,                                    vec2(tex_uv_r.y, tex_uv.y)).rgb;                                          vec3 new_sample2 = tex2Dlod0try(tex,                                    vec2(tex_uv_r.z, tex_uv.y)).rgb;                                          vec3 new_sample3 = tex2Dlod0try(tex,                                    vec2(tex_uv_r.w, tex_uv.y)).rgb;                                          UPDATE_COLOR_AND_WEIGHT_SUMS;

vec3 downsample_vertical_sinc_tiled(sampler2D tex, vec2 tex_uv, vec2 tex_size, float dr, float magnification_scale, float tile_size_uv_r)
{
    int samples = int(max_sinc_resize_samples_m4);

    float input_tiles_per_texture_r = 1.0/tile_size_uv_r;

    vec2 first_texel_tile_r_and_dist = get_first_texel_tile_uv_and_dist(tex_uv, tex_size, dr, input_tiles_per_texture_r, float(samples), true);
    vec4 first_texel_tile_uv_rrrr    = first_texel_tile_r_and_dist.xxxx;
    vec4 first_dist_unscaled         = first_texel_tile_r_and_dist.yyyy;

    float tile_dr = dr * input_tiles_per_texture_r;

    int i_base = 0;
    int i_step = 4;
    vec4 weight_sum  = vec4(0.0);
    vec3 pixel_color = vec3(0.0);

    for(int i = 0; i < samples; i += i_step)
    {
        VERTICAL_SINC_RESAMPLE_LOOP_BODY;
    }

    vec2 weight_sum_reduce = weight_sum.xy + weight_sum.zw;
    vec3 scalar_weight_sum = vec3(weight_sum_reduce.x + weight_sum_reduce.y);

    return (pixel_color/scalar_weight_sum);
}

vec3 downsample_horizontal_sinc_tiled(sampler2D tex, vec2 tex_uv, vec2 tex_size, float dr, float magnification_scale, float tile_size_uv_r)
{
    int samples = int(max_sinc_resize_samples_m4);

    float input_tiles_per_texture_r = 1.0/tile_size_uv_r;

    vec2 first_texel_tile_r_and_dist = get_first_texel_tile_uv_and_dist(tex_uv, tex_size, dr, input_tiles_per_texture_r, float(samples), false);
    vec4 first_texel_tile_uv_rrrr    = first_texel_tile_r_and_dist.xxxx;
    vec4 first_dist_unscaled         = first_texel_tile_r_and_dist.yyyy;

    float tile_dr = dr * input_tiles_per_texture_r;

    int i_base = 0;
    int i_step = 4;
    vec4 weight_sum  = vec4(0.0);
    vec3 pixel_color = vec3(0.0);

    for(int i = 0; i < samples; i += i_step)
    {
        HORIZONTAL_SINC_RESAMPLE_LOOP_BODY;
    }

    vec2 weight_sum_reduce = weight_sum.xy + weight_sum.zw;
    vec3 scalar_weight_sum = vec3(weight_sum_reduce.x + weight_sum_reduce.y);

    return (pixel_color/scalar_weight_sum);
}

vec2 get_resized_mask_tile_size(vec2 estimated_viewport_size, vec2 estimated_mask_resize_output_size, bool solemnly_swear_same_inputs_for_every_pass)
{

    float tile_aspect_ratio_inv = mask_resize_src_lut_size.y/mask_resize_src_lut_size.x;
    float tile_aspect_ratio     = 1.0/tile_aspect_ratio_inv;
    vec2  tile_aspect           = vec2(1.0, tile_aspect_ratio_inv);

    float desired_tile_size_x = mask_triads_per_tile * mix((3.0), estimated_viewport_size.x / (480.0), (0.0));

    float temp_tile_size_x = min(desired_tile_size_x, mask_resize_src_lut_size.x);

    vec2 temp_tile_size    = temp_tile_size_x * tile_aspect;
    vec2 min_tile_size     = mask_min_allowed_tile_size * tile_aspect;
    vec2 max_tile_size     = estimated_mask_resize_output_size / mask_resize_num_tiles;
    vec2 clamped_tile_size = clamp(temp_tile_size, min_tile_size, max_tile_size);

    float x_tile_size_from_y = clamped_tile_size.y * tile_aspect_ratio;
    float y_tile_size_from_x = mix(clamped_tile_size.y, clamped_tile_size.x * tile_aspect_ratio_inv, float(solemnly_swear_same_inputs_for_every_pass));
    vec2 reclamped_tile_size = vec2(min(clamped_tile_size.x, x_tile_size_from_y), min(clamped_tile_size.y, y_tile_size_from_x));

    vec2 final_resized_tile_size = floor(reclamped_tile_size + vec2(FIX_ZERO(0.0)));

    return final_resized_tile_size;
}

vec4 get_mask_sampling_parameters(vec2 mask_resize_texture_size, vec2 mask_resize_video_size, vec2 true_viewport_size, out vec2 mask_tiles_per_screen)
{
    vec2 mask_resize_tile_size = get_resized_mask_tile_size(true_viewport_size, mask_resize_video_size, false);

    vec2 mask_tile_uv_size  = mask_resize_tile_size / mask_resize_texture_size;
    vec2 skipped_tiles      = mask_start_texels/mask_resize_tile_size;
    vec2 mask_tile_start_uv = skipped_tiles * mask_tile_uv_size;

    mask_tiles_per_screen = true_viewport_size / mask_resize_tile_size;

    return vec4(mask_tile_start_uv, mask_tile_uv_size);
}

vec2 convert_phosphor_tile_uv_wrap_to_tex_uv(vec2 tile_uv_wrap, vec4 mask_tile_start_uv_and_size)
{
    vec2 tile_uv = fract(tile_uv_wrap);
    vec2 mask_tex_uv = mask_tile_start_uv_and_size.xy + tile_uv * mask_tile_start_uv_and_size.zw;

    return mask_tex_uv;
}

#endif

#ifdef VERTEX

in vec2 a_pos;
#define Position vec4(a_pos * 2.0 - 1.0, 0.0, 1.0)
#define TexCoord a_pos
out vec2 src_tex_uv_wrap;
out vec2 tile_uv_wrap;
out vec2 resize_magnification_scale;
out vec2 src_dxdy;
out vec2 tile_size_uv;
out vec2 input_tiles_per_texture;

void main()
{
    gl_Position = Position;
    vec2 tex_uv = TexCoord.xy;

    vec2 estimated_viewport_size = OutputSize.xy / mask_resize_viewport_scale;

    vec2 mask_resize_tile_size = get_resized_mask_tile_size(estimated_viewport_size, OutputSize.xy, false);

    vec2 output_tiles_this_pass = OutputSize.xy / mask_resize_tile_size;
    vec2 output_video_uv        = tex_uv;
    tile_uv_wrap                = output_video_uv * output_tiles_this_pass;

    vec2 input_tile_size    = vec2(min(mask_resize_src_lut_size.x, SourceSize.x), mask_resize_tile_size.y);
    tile_size_uv            = input_tile_size / SourceSize.xy;
    input_tiles_per_texture = SourceSize.xy / input_tile_size;

    src_tex_uv_wrap = tile_uv_wrap * tile_size_uv;

    resize_magnification_scale = mask_resize_tile_size / input_tile_size;
    src_dxdy                   = vec2(1.0/SourceSize.x, 0.0);
}

#else

in vec2 src_tex_uv_wrap;
in vec2 tile_uv_wrap;
in vec2 resize_magnification_scale;
in vec2 src_dxdy;
in vec2 tile_size_uv;
in vec2 input_tiles_per_texture;
out vec4 FragColor;
uniform sampler2D Source;
#define input_texture Source

void main()
{

    #ifdef PHOSPHOR_MASK_MANUALLY_RESIZE

        vec2 tile_uv_wrap = tile_uv_wrap;

        if(max(tile_uv_wrap.x, tile_uv_wrap.y) <= mask_resize_num_tiles)
        {
            float src_dx = src_dxdy.x;
            vec2 src_tex_uv = fract(src_tex_uv_wrap);
            vec3 pixel_color = downsample_horizontal_sinc_tiled(input_texture, src_tex_uv, SourceSize.xy, src_dxdy.x, resize_magnification_scale.x, tile_size_uv.x);

            FragColor = vec4(pixel_color, 1.0);
        }
        else
        {
            discard;
        }
    #else
        discard;
        FragColor = vec4(1.0);
    #endif
}

#endif
