// crt-royale-fast pass 4: crt-royale-scanlines-horizontal-apply-mask.slang
// crt-royale (C) 2014 TroggleMonkey, crt-royale-fast by Hyllian (2024), GPL-2.0-or-later.
// From libretro/slang-shaders crt/shaders/crt-royale/src-fast, converted to GLSL ES 3.00
// by tools/royale/convert.py (parameters fixed at their preset defaults).


uniform vec4 SourceSize;
uniform vec4 OriginalSize;
uniform vec4 OutputSize;
uniform vec4 VERTICAL_SCANLINESSize;
uniform vec4 MASK_RESIZESize;

#define VERTICAL_SCANLINEStexture VERTICAL_SCANLINES
#define VERTICAL_SCANLINEStexture_size VERTICAL_SCANLINESSize.xy
#define VERTICAL_SCANLINESvideo_size VERTICAL_SCANLINESSize.xy
#ifdef INTEGRATED_GRAPHICS_COMPATIBILITY_MODE
	#define MASK_RESIZEtexture Source
#else
	#define MASK_RESIZEtexture MASK_RESIZE
#endif
#define MASK_RESIZEtexture_size MASK_RESIZESize.xy
#define MASK_RESIZEvideo_size MASK_RESIZESize.xy

#define bloom_approx_scale_x (float(OutputSize.x / SourceSize.y))
#define max_viewport_size_x (float(1080.0*1024.0*(4.0/3.0)))

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

#ifndef SCANLINE_FUNCTIONS_H
#define SCANLINE_FUNCTIONS_H

#ifndef SPECIAL_FUNCTIONS_H
#define SPECIAL_FUNCTIONS_H

vec4 erf6(vec4 x)
{

	vec4 one = vec4(1.0);
	vec4 sign_x = sign(x);
	vec4 t = one/(one + 0.47047*abs(x));
	vec4 result = one - t*(0.3480242 + t*(-0.0958798 + t*0.7478556))*
		exp(-(x*x));
	return result * sign_x;
}

vec3 erf6(vec3 x)
{

	vec3 one = vec3(1.0);
	vec3 sign_x = sign(x);
	vec3 t = one/(one + 0.47047*abs(x));
	vec3 result = one - t*(0.3480242 + t*(-0.0958798 + t*0.7478556))*
		exp(-(x*x));
	return result * sign_x;
}

vec2 erf6(vec2 x)
{

	vec2 one = vec2(1.0);
	vec2 sign_x = sign(x);
	vec2 t = one/(one + 0.47047*abs(x));
	vec2 result = one - t*(0.3480242 + t*(-0.0958798 + t*0.7478556))*
		exp(-(x*x));
	return result * sign_x;
}

float erf6(float x)
{

	float sign_x = sign(x);
	float t = 1.0/(1.0 + 0.47047*abs(x));
	float result = 1.0 - t*(0.3480242 + t*(-0.0958798 + t*0.7478556))*
		exp(-(x*x));
	return result * sign_x;
}

vec4 erft(vec4 x)
{

	return tanh(1.202760580 * x);
}

vec3 erft(vec3 x)
{

	return tanh(1.202760580 * x);
}

vec2 erft(vec2 x)
{

	return tanh(1.202760580 * x);
}

float erft(float x)
{

	return tanh(1.202760580 * x);
}

vec4 erf(vec4 x)
{

	#ifdef ERF_FAST_APPROXIMATION
		return erft(x);
	#else
		return erf6(x);
	#endif
}

vec3 erf(vec3 x)
{

	#ifdef ERF_FAST_APPROXIMATION
		return erft(x);
	#else
		return erf6(x);
	#endif
}

vec2 erf(vec2 x)
{

	#ifdef ERF_FAST_APPROXIMATION
		return erft(x);
	#else
		return erf6(x);
	#endif
}

float erf(float x)
{

	#ifdef ERF_FAST_APPROXIMATION
		return erft(x);
	#else
		return erf6(x);
	#endif
}

vec4 gamma_impl(vec4 s, vec4 s_inv)
{

	vec4 g = vec4(1.12906830989);
	vec4 c0 = vec4(0.8109119309638332633713423362694399653724431);
	vec4 c1 = vec4(0.4808354605142681877121661197951496120000040);
	vec4 e = vec4(2.71828182845904523536028747135266249775724709);
	vec4 sph = s + vec4(0.5);
	vec4 lanczos_sum = c0 + c1/(s + vec4(1.0));
	vec4 base = (sph + g)/e;

	return (pow(base, sph) * lanczos_sum) * s_inv;
}

vec3 gamma_impl(vec3 s, vec3 s_inv)
{

	vec3 g = vec3(1.12906830989);
	vec3 c0 = vec3(0.8109119309638332633713423362694399653724431);
	vec3 c1 = vec3(0.4808354605142681877121661197951496120000040);
	vec3 e = vec3(2.71828182845904523536028747135266249775724709);
	vec3 sph = s + vec3(0.5);
	vec3 lanczos_sum = c0 + c1/(s + vec3(1.0));
	vec3 base = (sph + g)/e;
	return (pow(base, sph) * lanczos_sum) * s_inv;
}

vec2 gamma_impl(vec2 s, vec2 s_inv)
{

	vec2 g = vec2(1.12906830989);
	vec2 c0 = vec2(0.8109119309638332633713423362694399653724431);
	vec2 c1 = vec2(0.4808354605142681877121661197951496120000040);
	vec2 e = vec2(2.71828182845904523536028747135266249775724709);
	vec2 sph = s + vec2(0.5);
	vec2 lanczos_sum = c0 + c1/(s + vec2(1.0));
	vec2 base = (sph + g)/e;
	return (pow(base, sph) * lanczos_sum) * s_inv;
}

float gamma_impl(float s, float s_inv)
{

	float g = 1.12906830989;
	float c0 = 0.8109119309638332633713423362694399653724431;
	float c1 = 0.4808354605142681877121661197951496120000040;
	float e = 2.71828182845904523536028747135266249775724709;
	float sph = s + 0.5;
	float lanczos_sum = c0 + c1/(s + 1.0);
	float base = (sph + g)/e;
	return (pow(base, sph) * lanczos_sum) * s_inv;
}

vec4 gamma(vec4 s)
{

	return gamma_impl(s, vec4(1.0)/s);
}

vec3 gamma(vec3 s)
{

	return gamma_impl(s, vec3(1.0)/s);
}

vec2 gamma(vec2 s)
{

	return gamma_impl(s, vec2(1.0)/s);
}

float gamma(float s)
{

	return gamma_impl(s, 1.0/s);
}

vec4 ligamma_small_z_impl(vec4 s, vec4 z, vec4 s_inv)
{

	vec4 scale = pow(z, s);
	vec4 sum = s_inv;

	vec4 z_sq = z*z;
	vec4 denom1 = s + vec4(1.0);
	vec4 denom2 = 2.0*s + vec4(4.0);
	vec4 denom3 = 6.0*s + vec4(18.0);

	sum -= z/denom1;
	sum += z_sq/denom2;
	sum -= z * z_sq/denom3;

	return scale * sum;
}

vec3 ligamma_small_z_impl(vec3 s, vec3 z, vec3 s_inv)
{

	vec3 scale = pow(z, s);
	vec3 sum = s_inv;
	vec3 z_sq = z*z;
	vec3 denom1 = s + vec3(1.0);
	vec3 denom2 = 2.0*s + vec3(4.0);
	vec3 denom3 = 6.0*s + vec3(18.0);
	sum -= z/denom1;
	sum += z_sq/denom2;
	sum -= z * z_sq/denom3;
	return scale * sum;
}

vec2 ligamma_small_z_impl(vec2 s, vec2 z, vec2 s_inv)
{

	vec2 scale = pow(z, s);
	vec2 sum = s_inv;
	vec2 z_sq = z*z;
	vec2 denom1 = s + vec2(1.0);
	vec2 denom2 = 2.0*s + vec2(4.0);
	vec2 denom3 = 6.0*s + vec2(18.0);
	sum -= z/denom1;
	sum += z_sq/denom2;
	sum -= z * z_sq/denom3;
	return scale * sum;
}

float ligamma_small_z_impl(float s, float z, float s_inv)
{

	float scale = pow(z, s);
	float sum = s_inv;
	float z_sq = z*z;
	float denom1 = s + 1.0;
	float denom2 = 2.0*s + 4.0;
	float denom3 = 6.0*s + 18.0;
	sum -= z/denom1;
	sum += z_sq/denom2;
	sum -= z * z_sq/denom3;
	return scale * sum;
}

vec4 uigamma_large_z_impl(vec4 s, vec4 z)
{

	vec4 numerator = pow(z, s) * exp(-z);
	vec4 denom = vec4(7.0) + z - s;
	denom = vec4(5.0) + z - s + (3.0*s - vec4(9.0))/denom;
	denom = vec4(3.0) + z - s + (2.0*s - vec4(4.0))/denom;
	denom = vec4(1.0) + z - s + (s - vec4(1.0))/denom;
	return numerator / denom;
}

vec3 uigamma_large_z_impl(vec3 s, vec3 z)
{

	vec3 numerator = pow(z, s) * exp(-z);
	vec3 denom = vec3(7.0) + z - s;
	denom = vec3(5.0) + z - s + (3.0*s - vec3(9.0))/denom;
	denom = vec3(3.0) + z - s + (2.0*s - vec3(4.0))/denom;
	denom = vec3(1.0) + z - s + (s - vec3(1.0))/denom;
	return numerator / denom;
}

vec2 uigamma_large_z_impl(vec2 s, vec2 z)
{

	vec2 numerator = pow(z, s) * exp(-z);
	vec2 denom = vec2(7.0) + z - s;
	denom = vec2(5.0) + z - s + (3.0*s - vec2(9.0))/denom;
	denom = vec2(3.0) + z - s + (2.0*s - vec2(4.0))/denom;
	denom = vec2(1.0) + z - s + (s - vec2(1.0))/denom;
	return numerator / denom;
}

float uigamma_large_z_impl(float s, float z)
{

	float numerator = pow(z, s) * exp(-z);
	float denom = 7.0 + z - s;
	denom = 5.0 + z - s + (3.0*s - 9.0)/denom;
	denom = 3.0 + z - s + (2.0*s - 4.0)/denom;
	denom = 1.0 + z - s + (s - 1.0)/denom;
	return numerator / denom;
}

vec4 normalized_ligamma_impl(vec4 s, vec4 z,
    vec4 s_inv, vec4 gamma_s_inv)
{

	vec4 thresh = vec4(0.775075);
	bvec4 z_is_large;
	z_is_large.x = z.x > thresh.x;
	z_is_large.y = z.y > thresh.y;
	z_is_large.z = z.z > thresh.z;
	z_is_large.w = z.w > thresh.w;
	vec4 large_z = vec4(1.0) - uigamma_large_z_impl(s, z) * gamma_s_inv;
	vec4 small_z = ligamma_small_z_impl(s, z, s_inv) * gamma_s_inv;

	bvec4 inverse_z_is_large = not(z_is_large);
	return large_z * vec4(z_is_large) + small_z * vec4(inverse_z_is_large);
}

vec3 normalized_ligamma_impl(vec3 s, vec3 z,
    vec3 s_inv, vec3 gamma_s_inv)
{

	vec3 thresh = vec3(0.775075);
	bvec3 z_is_large;
	z_is_large.x = z.x > thresh.x;
	z_is_large.y = z.y > thresh.y;
	z_is_large.z = z.z > thresh.z;
	vec3 large_z = vec3(1.0) - uigamma_large_z_impl(s, z) * gamma_s_inv;
	vec3 small_z = ligamma_small_z_impl(s, z, s_inv) * gamma_s_inv;
	bvec3 inverse_z_is_large = not(z_is_large);
	return large_z * vec3(z_is_large) + small_z * vec3(inverse_z_is_large);
}

vec2 normalized_ligamma_impl(vec2 s, vec2 z,
    vec2 s_inv, vec2 gamma_s_inv)
{

	vec2 thresh = vec2(0.775075);
	bvec2 z_is_large;
	z_is_large.x = z.x > thresh.x;
	z_is_large.y = z.y > thresh.y;
	vec2 large_z = vec2(1.0) - uigamma_large_z_impl(s, z) * gamma_s_inv;
	vec2 small_z = ligamma_small_z_impl(s, z, s_inv) * gamma_s_inv;
	bvec2 inverse_z_is_large = not(z_is_large);
	return large_z * vec2(z_is_large) + small_z * vec2(inverse_z_is_large);
}

float normalized_ligamma_impl(float s, float z,
    float s_inv, float gamma_s_inv)
{

	float thresh = 0.775075;
	bool z_is_large = z > thresh;
	float large_z = 1.0 - uigamma_large_z_impl(s, z) * gamma_s_inv;
	float small_z = ligamma_small_z_impl(s, z, s_inv) * gamma_s_inv;
	return large_z * float(z_is_large) + small_z * float(!z_is_large);
}

vec4 normalized_ligamma(vec4 s, vec4 z)
{

	vec4 s_inv = vec4(1.0)/s;
	vec4 gamma_s_inv = vec4(1.0)/gamma_impl(s, s_inv);
	return normalized_ligamma_impl(s, z, s_inv, gamma_s_inv);
}

vec3 normalized_ligamma(vec3 s, vec3 z)
{

	vec3 s_inv = vec3(1.0)/s;
	vec3 gamma_s_inv = vec3(1.0)/gamma_impl(s, s_inv);
	return normalized_ligamma_impl(s, z, s_inv, gamma_s_inv);
}

vec2 normalized_ligamma(vec2 s, vec2 z)
{

	vec2 s_inv = vec2(1.0)/s;
	vec2 gamma_s_inv = vec2(1.0)/gamma_impl(s, s_inv);
	return normalized_ligamma_impl(s, z, s_inv, gamma_s_inv);
}

float normalized_ligamma(float s, float z)
{

	float s_inv = 1.0/s;
	float gamma_s_inv = 1.0/gamma_impl(s, s_inv);
	return normalized_ligamma_impl(s, z, s_inv, gamma_s_inv);
}

#endif

vec3 get_gaussian_sigma(vec3 color, float sigma_range)
{
    if(beam_spot_shape_function < 0.5)
    {

        return vec3(beam_min_sigma) + sigma_range * pow(color, vec3(beam_spot_power));
    }
    else
    {

        vec3 color_minus_1 = color - vec3(1.0);

	return vec3(beam_min_sigma) + sigma_range * sqrt(vec3(1.0) - color_minus_1*color_minus_1);
    }
}

vec3 get_generalized_gaussian_beta(vec3 color, float shape_range)
{
    return beam_min_shape + shape_range * pow(color, vec3(beam_shape_power));
}

vec3 scanline_gaussian_integral_contrib(vec3 dist, vec3 color, float pixel_height, float sigma_range)
{
    vec3 sigma         = get_gaussian_sigma(color, sigma_range);
    vec3 ph_offset     = vec3(pixel_height * 0.5);
    vec3 denom_inv     = 1.0/(sigma*sqrt(2.0));
    vec3 integral_high = erf((dist + ph_offset)*denom_inv);
    vec3 integral_low  = erf((dist - ph_offset)*denom_inv);

    return color * 0.5*(integral_high - integral_low)/pixel_height;
}

vec3 scanline_generalized_gaussian_integral_contrib(vec3 dist, vec3 color, float pixel_height, float sigma_range, float shape_range)
{
    vec3 alpha     = sqrt(2.0) * get_gaussian_sigma(color, sigma_range);
    vec3 beta      = get_generalized_gaussian_beta(color, shape_range);
    vec3 alpha_inv = vec3(1.0)/alpha;
    vec3 s         = vec3(1.0)/beta;
    vec3 ph_offset = vec3(pixel_height * 0.5);

    vec3 gamma_s_inv   = vec3(1.0)/gamma_impl(s, beta);
    vec3 dist1         = dist + ph_offset;
    vec3 dist0         = dist - ph_offset;
    vec3 integral_high = sign(dist1) * normalized_ligamma_impl(s, pow(abs(dist1)*alpha_inv, beta), beta, gamma_s_inv);
    vec3 integral_low  = sign(dist0) * normalized_ligamma_impl(s, pow(abs(dist0)*alpha_inv, beta), beta, gamma_s_inv);

    return color * 0.5*(integral_high - integral_low)/pixel_height;
}

vec3 scanline_gaussian_sampled_contrib(vec3 dist, vec3 color, float pixel_height, float sigma_range)
{
    vec3 sigma     = get_gaussian_sigma(color, sigma_range);
    vec3 sigma_inv = vec3(1.0)/sigma;

    vec3 inner_denom_inv = 0.5 * sigma_inv * sigma_inv;
    vec3 outer_denom_inv = sigma_inv/sqrt(2.0*pi);

    if(beam_antialias_level > 0.5)
    {

        vec3 sample_offset = vec3(pixel_height/3.0);
        vec3 dist2         = dist + sample_offset;
        vec3 dist3         = abs(dist - sample_offset);

        vec3 scale   = color/3.0  * outer_denom_inv;
        vec3 weight1 = exp(-( dist* dist)*inner_denom_inv);
        vec3 weight2 = exp(-(dist2*dist2)*inner_denom_inv);
        vec3 weight3 = exp(-(dist3*dist3)*inner_denom_inv);

	return scale * (weight1 + weight2 + weight3);
    }
    else
    {
        return color*exp(-(dist*dist)*inner_denom_inv)*outer_denom_inv;
    }
}

vec3 scanline_generalized_gaussian_sampled_contrib(vec3 dist, vec3 color, float pixel_height, float sigma_range, float shape_range)
{
    vec3 alpha = sqrt(2.0) * get_gaussian_sigma(color, sigma_range);
    vec3 beta  = get_generalized_gaussian_beta(color, shape_range);

    vec3 alpha_inv = vec3(1.0)/alpha;
    vec3 beta_inv  = vec3(1.0)/beta;
    vec3 scale     = color * beta * 0.5 * alpha_inv / gamma_impl(beta_inv, beta);

    if(beam_antialias_level > 0.5)
    {

        vec3 sample_offset = vec3(pixel_height/3.0);
        vec3 dist2         = dist + sample_offset;
        vec3 dist3         = abs(dist - sample_offset);

        vec3 weight1 = exp(-pow(abs( dist*alpha_inv), beta));
        vec3 weight2 = exp(-pow(abs(dist2*alpha_inv), beta));
        vec3 weight3 = exp(-pow(abs(dist3*alpha_inv), beta));

	return scale/3.0 * (weight1 + weight2 + weight3);
    }
    else
    {
        return scale * exp(-pow(abs(dist*alpha_inv), beta));
    }
}

vec3 scanline_contrib(vec3 dist, vec3 color, float pixel_height, float sigma_range, float shape_range)
{
    if(beam_generalized_gaussian)
    {
        if(beam_antialias_level > 1.5)
        {
            return scanline_generalized_gaussian_integral_contrib(dist, color, pixel_height, sigma_range, shape_range);
        }
        else
        {
            return scanline_generalized_gaussian_sampled_contrib(dist, color, pixel_height, sigma_range, shape_range);
        }
    }
    else
    {
        if(beam_antialias_level > 1.5)
        {
            return scanline_gaussian_integral_contrib(dist, color, pixel_height, sigma_range);
        }
        else
        {
            return scanline_gaussian_sampled_contrib(dist, color, pixel_height, sigma_range);
        }
    }
}

vec3 get_raw_interpolated_color(vec3 color0, vec3 color1, vec3 color2, vec3 color3, vec4 weights)
{

    return max((mat4x3(color0, color1, color2, color3) * weights), 0.0);
}

vec3 get_interpolated_linear_color(vec3 color0, vec3 color1, vec3 color2, vec3 color3, vec4 weights)
{
    float intermediate_gamma = lcd_gamma;

            vec3 linear_mixed_color = get_raw_interpolated_color(color0, color1, color2, color3, weights);

	    vec3 gamma_mixed_color = get_raw_interpolated_color(
                    pow(color0, vec3(1.0/intermediate_gamma)),
                    pow(color1, vec3(1.0/intermediate_gamma)),
                    pow(color2, vec3(1.0/intermediate_gamma)),
                    pow(color3, vec3(1.0/intermediate_gamma)),
                    weights);

            return mix(gamma_mixed_color, linear_mixed_color, (1.0));
}

vec3 get_scanline_color(sampler2D tex, vec2 scanline_uv, vec2 uv_step_x, vec4 weights)
{
    vec3 color1 = texture(tex, scanline_uv).rgb;
    vec3 color2 = texture(tex, scanline_uv + uv_step_x).rgb;
    vec3 color0 = vec3(0.0);
    vec3 color3 = vec3(0.0);

    if(beam_horiz_filter > 0.5)
    {
        color0 = texture(tex, scanline_uv - uv_step_x).rgb;
        color3 = texture(tex, scanline_uv + 2.0 * uv_step_x).rgb;
    }

    return get_interpolated_linear_color(color0, color1, color2, color3, weights);
}

vec3 sample_single_scanline_horizontal(sampler2D tex, vec2 tex_uv, vec2 tex_size, vec2 texture_size_inv)
{
    vec2 curr_texel = tex_uv * tex_size;

    vec2 prev_texel        = floor(curr_texel - vec2(under_half)) + vec2(0.5);
    vec2 prev_texel_hor    = vec2(prev_texel.x, curr_texel.y);
    vec2 prev_texel_hor_uv = prev_texel_hor * texture_size_inv;

    float prev_dist   = curr_texel.x - prev_texel_hor.x;
    vec4 sample_dists = vec4(1.0 + prev_dist, prev_dist, 1.0 - prev_dist, 2.0 - prev_dist);

    vec4 weights;

    if(beam_horiz_filter < 0.5)
    {

        float x  = sample_dists.y;
        float w2 = x*x*x*(x*(x*6.0 - 15.0) + 10.0);
        weights  = vec4(0.0, 1.0 - w2, w2, 0.0);
    }
    else if(beam_horiz_filter < 1.5)
    {

        float inner_denom_inv = 1.0/(2.0*beam_horiz_sigma*beam_horiz_sigma);
        weights               = exp(-(sample_dists*sample_dists)*inner_denom_inv);
    }
    else if(beam_horiz_filter < 2.5)
    {

        vec4 pi_dists = FIX_ZERO(sample_dists * pi);
        weights       = 2.0 * sin(pi_dists) * sin(pi_dists * 0.5)/(pi_dists * pi_dists);
    }
    else
    {

        float x  = sample_dists.y;
        weights  = vec4(0.0, 1.0 - x, x, 0.0);
    }

    vec4 final_weights = weights/dot(weights, vec4(1.0));

    vec2 uv_step_x = vec2(texture_size_inv.x, 0.0);

    return get_scanline_color(tex, prev_texel_hor_uv, uv_step_x, final_weights);
}

vec3 sample_rgb_scanline_horizontal(sampler2D tex, vec2 tex_uv, vec2 tex_size, vec2 texture_size_inv)
{

    if(beam_misconvergence)
    {
        vec3 convergence_offsets_rgb = get_convergence_offsets_x_vector();
        vec3 offset_u_rgb = convergence_offsets_rgb * texture_size_inv.xxx;

	vec2 scanline_uv_r = tex_uv - vec2(offset_u_rgb.r, 0.0);
        vec2 scanline_uv_g = tex_uv - vec2(offset_u_rgb.g, 0.0);
        vec2 scanline_uv_b = tex_uv - vec2(offset_u_rgb.b, 0.0);

	vec3 sample_r = sample_single_scanline_horizontal(tex, scanline_uv_r, tex_size, texture_size_inv);
        vec3 sample_g = sample_single_scanline_horizontal(tex, scanline_uv_g, tex_size, texture_size_inv);
        vec3 sample_b = sample_single_scanline_horizontal(tex, scanline_uv_b, tex_size, texture_size_inv);

	return vec3(sample_r.r, sample_g.g, sample_b.b);
    }
    else
    {
        return sample_single_scanline_horizontal(tex, tex_uv, tex_size, texture_size_inv);
    }
}

vec2 get_last_scanline_uv( vec2 tex_uv,
		           vec2 tex_size, vec2 texture_size_inv,
		           vec2 il_step_multiple,
                           float frame_count, out float dist)
{
    float field_offset = floor(il_step_multiple.y * 0.75)*mod(frame_count + float((0.0)), 2.0);
    vec2  curr_texel   = tex_uv * tex_size;

    vec2  prev_texel_num     = floor(curr_texel - vec2(under_half));
    float wrong_field        = mod(prev_texel_num.y + field_offset, il_step_multiple.y);
    vec2  scanline_texel_num = prev_texel_num - vec2(0.0, wrong_field);

    vec2  scanline_texel = scanline_texel_num + vec2(0.5);
    vec2  scanline_uv    = scanline_texel * texture_size_inv;

    dist = (curr_texel.y - scanline_texel.y)/il_step_multiple.y;

    return scanline_uv;
}

bool is_interlaced(float num_lines)
{

    if(interlace_detect)
    {

        bool sd_interlace = ((num_lines > 288.5) && (num_lines < 576.5));
        bool hd_interlace = bool(interlace_1080i) ? ((num_lines > 1079.5) && (num_lines < 1080.5)) : false;

	return (sd_interlace || hd_interlace);
    }
    else
    {
        return false;
    }
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

#define FIX(c) max(abs(c), 1e-5);
#define PI 3.141592653589

#define aspect (vec2(vec2((0.0) > 0.5 ? (0.75, 1.0) : (1.0, 0.75))))
#define overscan (vec2(vec2(1.01, 1.01)))

#ifdef VERTEX

in vec2 a_pos;
#define Position vec4(a_pos * 2.0 - 1.0, 0.0, 1.0)
#define TexCoord a_pos
out vec2 video_uv;
out vec2 scanline_texture_size_inv;
out vec4 mask_tile_start_uv_and_size;
out vec2 mask_tiles_per_screen;
out vec2 sinangle;
out vec2 cosangle;
out vec3 stretch;
out float R_d_cx_cy;
out float d2;

float intersect(vec2 xy)
{
    float A = dot(xy,xy) + (1.5)*(1.5);
    float B = 2.0*((4.0)*(dot(xy,sinangle)-(1.5)*cosangle.x*cosangle.y)-(1.5)*(1.5));
    float C = (1.5)*(1.5) + 2.0*(4.0)*(1.5)*cosangle.x*cosangle.y;

    return (-B-sqrt(B*B-4.0*A*C))/(2.0*A);
}

vec2 bkwtrans(vec2 xy)
{
    float c     = intersect(xy);
    vec2 point  = (vec2(c, c)*xy - vec2(-(4.0), -(4.0))*sinangle) / vec2((4.0), (4.0));
    vec2 poc    = point/cosangle;

    vec2 tang   = sinangle/cosangle;
    float A     = dot(tang, tang) + 1.0;
    float B     = -2.0*dot(poc, tang);
    float C     = dot(poc, poc) - 1.0;

    float a     = (-B + sqrt(B*B - 4.0*A*C))/(2.0*A);
    vec2 uv     = (point - a*sinangle)/cosangle;
    float r     = FIX((4.0)*acos(a));

    return uv*r/sin(r/(4.0));
}

vec2 fwtrans(vec2 uv)
{
    float r = FIX(sqrt(dot(uv,uv)));
    uv *= sin(r/(4.0))/r;
    float x = 1.0-cos(r/(4.0));
    float D = (1.5)/(4.0) + x*cosangle.x*cosangle.y+dot(uv,sinangle);

    return (1.5)*(uv*cosangle-x*sinangle)/D;
}

vec3 maxscale()
{
    vec2 c  = bkwtrans(-(4.0) * sinangle / (1.0 + (4.0)/(1.5)*cosangle.x*cosangle.y));
    vec2 a  = vec2(0.5,0.5)*aspect;

    vec2 lo = vec2(fwtrans(vec2(-a.x,  c.y)).x,
                   fwtrans(vec2( c.x, -a.y)).y)/aspect;

    vec2 hi = vec2(fwtrans(vec2(+a.x,  c.y)).x,
                   fwtrans(vec2( c.x, +a.y)).y)/aspect;

    return vec3((hi+lo)*aspect*0.5,max(hi.x-lo.x,hi.y-lo.y));
}

void main()
{
   gl_Position = Position;

    video_uv = TexCoord * vec2(1.0001) - vec2((0.0), (0.0));

    sinangle    = sin(vec2((0.0), (0.0)));
    cosangle    = cos(vec2((0.0), (0.0)));
    stretch     = maxscale();

    d2        = (1.5) * (1.5);
    R_d_cx_cy = (4.0) * (1.5) * cosangle.x * cosangle.y;

    scanline_texture_size_inv = vec2(1.0, 1.0)/VERTICAL_SCANLINEStexture_size;

    #ifdef PHOSPHOR_MASK_MANUALLY_RESIZE
        vec2 mask_resize_texture_size = MASK_RESIZEtexture_size;
        vec2 mask_resize_video_size = MASK_RESIZEvideo_size;
    #endif

    mask_tile_start_uv_and_size = get_mask_sampling_parameters(mask_resize_texture_size, mask_resize_video_size, OutputSize.xy, mask_tiles_per_screen);
}

#else

in vec2 video_uv;
in vec2 scanline_texture_size_inv;
in vec4 mask_tile_start_uv_and_size;
in vec2 mask_tiles_per_screen;
in vec2 sinangle;
in vec2 cosangle;
in vec3 stretch;
in float R_d_cx_cy;
in float d2;
out vec4 FragColor;
uniform sampler2D Source;
uniform sampler2D VERTICAL_SCANLINES;
#ifdef PHOSPHOR_MASK_MANUALLY_RESIZE
uniform sampler2D MASK_RESIZE;
#endif

float intersect(vec2 xy, float d2, float R_d_cx_cy)
{
    float A = dot(xy,xy) + d2;
    float B = 2.0 * ( (4.0) * dot(xy, sinangle) - R_d_cx_cy - d2);
    float C = d2 + 2.0 * R_d_cx_cy;

    return (-B-sqrt(B*B-4.0*A*C))/(2.0*A);
}

vec2 bkwtrans(vec2 xy, float d2, float R_d_cx_cy)
{
    float c     = intersect(xy, d2, R_d_cx_cy);
    vec2 point  = (vec2(c, c)*xy - vec2(-(4.0), -(4.0))*sinangle) / vec2((4.0), (4.0));
    vec2 poc    = point/cosangle;

    vec2 tang   = sinangle/cosangle;
    float A     = dot(tang, tang) + 1.0;
    float B     = -2.0*dot(poc, tang);
    float C     = dot(poc, poc) - 1.0;

    float a     = (-B + sqrt(B*B - 4.0*A*C))/(2.0*A);
    vec2 uv     = (point - a*sinangle)/cosangle;
    float r     = FIX((4.0)*acos(a));

    return uv*r/sin(r/(4.0));
}

vec2 transform(vec2 coord, float d2, float R_d_cx_cy)
{
    coord = (coord - vec2(0.5, 0.5))*aspect*stretch.z + stretch.xy;

    return (bkwtrans(coord, d2, R_d_cx_cy) /
        vec2((100.0) / 100.0, (100.0) / 100.0)/aspect + vec2(0.5, 0.5));
}

float corner(vec2 coord)
{
    coord = (coord - vec2(0.5)) * vec2((100.0) / 100.0, (100.0) / 100.0) + vec2(0.5, 0.5);
    coord = min(coord, vec2(1.0) - coord) * aspect;
    vec2 cdist = vec2((0.006));
    coord = (cdist - min(coord, cdist));
    float dist = sqrt(dot(coord, coord));

      return clamp((cdist.x - dist)*(400.0), 0.0, 1.0);
}

void main()
{

    vec2  uv   = video_uv;
    float cval = 1.0;

    if ((0.0) > 0.5)
    {
        uv   = transform(video_uv, d2, R_d_cx_cy);
        cval = corner(uv);
    }

    vec3 scanline_color_dim = sample_rgb_scanline_horizontal(VERTICAL_SCANLINEStexture, uv, VERTICAL_SCANLINEStexture_size, scanline_texture_size_inv);

    vec2 tile_uv_wrap = video_uv * mask_tiles_per_screen;

    vec2 mask_tex_uv = convert_phosphor_tile_uv_wrap_to_tex_uv(tile_uv_wrap, mask_tile_start_uv_and_size);

    vec3 phosphor_mask_sample;

    phosphor_mask_sample = texture(MASK_RESIZEtexture, mask_tex_uv).rgb;

    vec3 phosphor_emission_dim = scanline_color_dim * phosphor_mask_sample * vec3(cval);

    phosphor_emission_dim *= step(0.0, fract(uv.y));

    FragColor = vec4(phosphor_emission_dim, 1.0);
}

#endif
