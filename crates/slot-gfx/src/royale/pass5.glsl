// crt-royale-fast pass 5: crt-royale-brightpass.slang
// crt-royale (C) 2014 TroggleMonkey, crt-royale-fast by Hyllian (2024), GPL-2.0-or-later.
// From libretro/slang-shaders crt/shaders/crt-royale/src-fast, converted to GLSL ES 3.00
// by tools/royale/convert.py (parameters fixed at their preset defaults).


uniform vec4 SourceSize;
uniform vec4 OriginalSize;
uniform vec4 OutputSize;
uniform float FrameCount;
uniform vec4 MASKED_SCANLINESSize;

#define MASKED_SCANLINEStexture MASKED_SCANLINES
#define MASKED_SCANLINEStexture_size MASKED_SCANLINESSize.xy
#define MASKED_SCANLINESvideo_size MASKED_SCANLINESSize.xy

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

#define bloom_approx_scale_x (float(OutputSize.x / SourceSize.y))
#define max_viewport_size_x (float(1080.0*1024.0*(4.0/3.0)))
#define bloom_diff_thresh_ (float(1.0/256.0))
#define mask_min_allowed_tile_size (float(ceil(mask_min_allowed_triad_size * mask_triads_per_tile)))

#ifdef VERTEX

in vec2 a_pos;
#define Position vec4(a_pos * 2.0 - 1.0, 0.0, 1.0)
#define TexCoord a_pos
out vec2 tex_uv;
out float center_weight;
out float undim_mask_contrast_factors;

float get_min_sigma_to_blur_triad(float triad_size,
    float thresh)
{

    return -0.05168 + 0.6113*triad_size - 1.122*triad_size*sqrt(0.000416 + thresh);

}

float get_fast_gaussian_weight_sum_inv(float sigma)
{

    return min(exp(exp(0.348348412457428/(sigma - 0.0860587260734721))), 0.399334576340352/sigma);
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

void main()
{
    gl_Position = Position;

    tex_uv = TexCoord;

    float mask_tile_size_x    = get_resized_mask_tile_size(OutputSize.xy, OutputSize.xy * mask_resize_viewport_scale, false).x;
    float bloom_sigma_runtime = get_min_sigma_to_blur_triad(mask_tile_size_x / mask_triads_per_tile, bloom_diff_thresh_);

    center_weight = get_fast_gaussian_weight_sum_inv(bloom_sigma_runtime);

    float undim_factor = 1.0/levels_autodim_temp;

    undim_mask_contrast_factors = undim_factor * get_mask_amplify() * levels_contrast;
}

#else

in vec2 tex_uv;
in float center_weight;
in float undim_mask_contrast_factors;
out vec4 FragColor;
uniform sampler2D MASKED_SCANLINES;
uniform sampler2D ORIG_LINEARIZED;

void main()
{

    vec3 intensity_dim = texture(MASKED_SCANLINEStexture, tex_uv).rgb;

    vec3  intensity = intensity_dim * undim_mask_contrast_factors;

    vec3 phosphor_blur_approx = levels_contrast * texture(ORIG_LINEARIZED, tex_uv).rgb;

    vec3  max_area_contribution_approx = max(vec3(0.0, 0.0, 0.0), phosphor_blur_approx - center_weight * intensity);

    vec3 area_contrib_underestimate = bloom_underestimate_levels * max_area_contribution_approx;
    vec3 intensity_underestimate    = bloom_underestimate_levels * intensity;

    vec3 blur_ratio_temp = ((vec3(1.0, 1.0, 1.0) - area_contrib_underestimate) / intensity_underestimate - vec3(1.0, 1.0, 1.0)) / (center_weight - 1.0);
    vec3 blur_ratio      = clamp(blur_ratio_temp, 0.0, 1.0);

    vec3 brightpass = intensity_dim * mix(blur_ratio, vec3(1.0, 1.0, 1.0), (0.0));

    FragColor = vec4(brightpass, 1.0);
}

#endif
