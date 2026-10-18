-- Classes of the patterns of REGEXP over the code points, ICU (MySQL) against PCRE2 (MariaDB):
-- for each form, the pattern of MySQL and the one of MariaDB that the translation writes for it,
-- or the same pattern when it is copied, anchored, each server reading its own; under
-- `utf8mb4_unicode_ci` or `utf8mb4_bin`. The code points are those of planes 0 to 3 and 14
-- that MySQL 8.0.11 and 8.4.11, MariaDB 10.6.28 and 11.8.9 all assign (`\P{Cn}`, 134637 of them);
-- each form gives the count of the code points it matches and a hash of them. Measured: the
-- same in MySQL 8.4.11 and MariaDB, except the categories L, Lu, Ll, Lt, M and Mn under
-- `_ci`, which ICU closes over the case (refused by the translation), Mn and Mc (U+1171E, Mc
-- for ICU), and a range holding U+0390 without U+1FD3 under `_ci` (r_greek_ext, refused by
-- the translation); MySQL 8.0.11 otherwise for the code points of its version of Unicode
-- (docs/DEV-DIALECTS.md, section 6.3).
SET SESSION group_concat_max_len = 100000000;
DROP TABLE IF EXISTS rx_digits, rx_unassigned, rx_points;
CREATE TABLE rx_digits (n INT NOT NULL);
INSERT INTO rx_digits VALUES (0), (1), (2), (3), (4), (5), (6), (7), (8), (9), (10), (11), (12), (13), (14), (15);
-- Code points that a server leaves unassigned, as ranges.
CREATE TABLE rx_unassigned (lo INT NOT NULL PRIMARY KEY, hi INT NOT NULL);
INSERT INTO rx_unassigned VALUES (888, 889), (896, 899), (907, 907), (909, 909), (930, 930), (1328, 1328), (1367, 1368), (1376, 1376), (1416, 1416), (1419, 1420), (1424, 1424), (1480, 1487);
INSERT INTO rx_unassigned VALUES (1515, 1519), (1525, 1535), (1565, 1565), (1806, 1806), (1867, 1868), (1970, 1983), (2043, 2047), (2094, 2095), (2111, 2111), (2140, 2141), (2143, 2207), (2229, 2229);
INSERT INTO rx_unassigned VALUES (2238, 2259), (2436, 2436), (2445, 2446), (2449, 2450), (2473, 2473), (2481, 2481), (2483, 2485), (2490, 2491), (2501, 2502), (2505, 2506), (2511, 2518), (2520, 2523);
INSERT INTO rx_unassigned VALUES (2526, 2526), (2532, 2533), (2556, 2560), (2564, 2564), (2571, 2574), (2577, 2578), (2601, 2601), (2609, 2609), (2612, 2612), (2615, 2615), (2618, 2619), (2621, 2621);
INSERT INTO rx_unassigned VALUES (2627, 2630), (2633, 2634), (2638, 2640), (2642, 2648), (2653, 2653), (2655, 2661), (2678, 2688), (2692, 2692), (2702, 2702), (2706, 2706), (2729, 2729), (2737, 2737);
INSERT INTO rx_unassigned VALUES (2740, 2740), (2746, 2747), (2758, 2758), (2762, 2762), (2766, 2767), (2769, 2783), (2788, 2789), (2802, 2808), (2810, 2816), (2820, 2820), (2829, 2830), (2833, 2834);
INSERT INTO rx_unassigned VALUES (2857, 2857), (2865, 2865), (2868, 2868), (2874, 2875), (2885, 2886), (2889, 2890), (2894, 2901), (2904, 2907), (2910, 2910), (2916, 2917), (2936, 2945), (2948, 2948);
INSERT INTO rx_unassigned VALUES (2955, 2957), (2961, 2961), (2966, 2968), (2971, 2971), (2973, 2973), (2976, 2978), (2981, 2983), (2987, 2989), (3002, 3005), (3011, 3013), (3017, 3017), (3022, 3023);
INSERT INTO rx_unassigned VALUES (3025, 3030), (3032, 3045), (3067, 3071), (3076, 3076), (3085, 3085), (3089, 3089), (3113, 3113), (3130, 3132), (3141, 3141), (3145, 3145), (3150, 3156), (3159, 3159);
INSERT INTO rx_unassigned VALUES (3163, 3167), (3172, 3173), (3184, 3191), (3204, 3204), (3213, 3213), (3217, 3217), (3241, 3241), (3252, 3252), (3258, 3259), (3269, 3269), (3273, 3273), (3278, 3284);
INSERT INTO rx_unassigned VALUES (3287, 3293), (3295, 3295), (3300, 3301), (3312, 3312), (3315, 3328), (3332, 3332), (3341, 3341), (3345, 3345), (3387, 3388), (3397, 3397), (3401, 3401), (3408, 3411);
INSERT INTO rx_unassigned VALUES (3428, 3429), (3456, 3457), (3460, 3460), (3479, 3481), (3506, 3506), (3516, 3516), (3518, 3519), (3527, 3529), (3531, 3534), (3541, 3541), (3543, 3543), (3552, 3557);
INSERT INTO rx_unassigned VALUES (3568, 3569), (3573, 3584), (3643, 3646), (3676, 3712), (3715, 3715), (3717, 3718), (3721, 3721), (3723, 3724), (3726, 3731), (3736, 3736), (3744, 3744), (3748, 3748);
INSERT INTO rx_unassigned VALUES (3750, 3750), (3752, 3753), (3756, 3756), (3770, 3770), (3774, 3775), (3781, 3781), (3783, 3783), (3790, 3791), (3802, 3803), (3808, 3839), (3912, 3912), (3949, 3952);
INSERT INTO rx_unassigned VALUES (3992, 3992), (4029, 4029), (4045, 4045), (4059, 4095), (4294, 4294), (4296, 4300), (4302, 4303), (4681, 4681), (4686, 4687), (4695, 4695), (4697, 4697), (4702, 4703);
INSERT INTO rx_unassigned VALUES (4745, 4745), (4750, 4751), (4785, 4785), (4790, 4791), (4799, 4799), (4801, 4801), (4806, 4807), (4823, 4823), (4881, 4881), (4886, 4887), (4955, 4956), (4989, 4991);
INSERT INTO rx_unassigned VALUES (5018, 5023), (5110, 5111), (5118, 5119), (5789, 5791), (5881, 5887), (5901, 5901), (5909, 5919), (5943, 5951), (5972, 5983), (5997, 5997), (6001, 6001), (6004, 6015);
INSERT INTO rx_unassigned VALUES (6110, 6111), (6122, 6127), (6138, 6143), (6159, 6159), (6170, 6175), (6264, 6271), (6315, 6319), (6390, 6399), (6431, 6431), (6444, 6447), (6460, 6463), (6465, 6467);
INSERT INTO rx_unassigned VALUES (6510, 6511), (6517, 6527), (6572, 6575), (6602, 6607), (6619, 6621), (6684, 6685), (6751, 6751), (6781, 6782), (6794, 6799), (6810, 6815), (6830, 6831), (6847, 6911);
INSERT INTO rx_unassigned VALUES (6988, 6991), (7037, 7039), (7156, 7163), (7224, 7226), (7242, 7244), (7305, 7359), (7368, 7375), (7415, 7415), (7418, 7423), (7670, 7674), (7958, 7959), (7966, 7967);
INSERT INTO rx_unassigned VALUES (8006, 8007), (8014, 8015), (8024, 8024), (8026, 8026), (8028, 8028), (8030, 8030), (8062, 8063), (8117, 8117), (8133, 8133), (8148, 8149), (8156, 8156), (8176, 8177);
INSERT INTO rx_unassigned VALUES (8181, 8181), (8191, 8191), (8293, 8293), (8306, 8307), (8335, 8335), (8349, 8351), (8383, 8399), (8433, 8447), (8588, 8591), (9215, 9215), (9255, 9279), (9291, 9311);
INSERT INTO rx_unassigned VALUES (11124, 11125), (11158, 11159), (11194, 11196), (11209, 11209), (11218, 11243), (11248, 11263), (11311, 11311), (11359, 11359), (11508, 11512), (11558, 11558), (11560, 11564), (11566, 11567);
INSERT INTO rx_unassigned VALUES (11624, 11630), (11633, 11646), (11671, 11679), (11687, 11687), (11695, 11695), (11703, 11703), (11711, 11711), (11719, 11719), (11727, 11727), (11735, 11735), (11743, 11743), (11845, 11903);
INSERT INTO rx_unassigned VALUES (11930, 11930), (12020, 12031), (12246, 12271), (12284, 12287), (12352, 12352), (12439, 12440), (12544, 12548), (12590, 12592), (12687, 12687), (12731, 12735), (12772, 12783), (12831, 12831);
INSERT INTO rx_unassigned VALUES (13055, 13055), (19894, 19903), (40918, 40959), (42125, 42127), (42183, 42191), (42540, 42559), (42744, 42751), (42927, 42927), (42936, 42998), (43052, 43055), (43066, 43071), (43128, 43135);
INSERT INTO rx_unassigned VALUES (43206, 43213), (43226, 43231), (43262, 43263), (43348, 43358), (43389, 43391), (43470, 43470), (43482, 43485), (43519, 43519), (43575, 43583), (43598, 43599), (43610, 43611), (43715, 43738);
INSERT INTO rx_unassigned VALUES (43767, 43776), (43783, 43784), (43791, 43792), (43799, 43807), (43815, 43815), (43823, 43823), (43878, 43887), (44014, 44015), (44026, 44031), (55204, 55215), (55239, 55242), (55292, 55295);
INSERT INTO rx_unassigned VALUES (64110, 64111), (64218, 64255), (64263, 64274), (64280, 64284), (64311, 64311), (64317, 64317), (64319, 64319), (64322, 64322), (64325, 64325), (64450, 64466), (64832, 64847), (64912, 64913);
INSERT INTO rx_unassigned VALUES (64968, 65007), (65022, 65023), (65050, 65055), (65107, 65107), (65127, 65127), (65132, 65135), (65141, 65141), (65277, 65278), (65280, 65280), (65471, 65473), (65480, 65481), (65488, 65489);
INSERT INTO rx_unassigned VALUES (65496, 65497), (65501, 65503), (65511, 65511), (65519, 65528), (65534, 65535), (65548, 65548), (65575, 65575), (65595, 65595), (65598, 65598), (65614, 65615), (65630, 65663), (65787, 65791);
INSERT INTO rx_unassigned VALUES (65795, 65798), (65844, 65846), (65935, 65935), (65948, 65951), (65953, 65999), (66046, 66175), (66205, 66207), (66257, 66271), (66300, 66303), (66340, 66351), (66379, 66383), (66427, 66431);
INSERT INTO rx_unassigned VALUES (66462, 66462), (66500, 66503), (66518, 66559), (66718, 66719), (66730, 66735), (66772, 66775), (66812, 66815), (66856, 66863), (66916, 66926), (66928, 67071), (67383, 67391), (67414, 67423);
INSERT INTO rx_unassigned VALUES (67432, 67583), (67590, 67591), (67593, 67593), (67638, 67638), (67641, 67643), (67645, 67646), (67670, 67670), (67743, 67750), (67760, 67807), (67827, 67827), (67830, 67834), (67868, 67870);
INSERT INTO rx_unassigned VALUES (67898, 67902), (67904, 67967), (68024, 68027), (68048, 68049), (68100, 68100), (68103, 68107), (68116, 68116), (68120, 68120), (68148, 68151), (68155, 68158), (68168, 68175), (68185, 68191);
INSERT INTO rx_unassigned VALUES (68256, 68287), (68327, 68330), (68343, 68351), (68406, 68408), (68438, 68439), (68467, 68471), (68498, 68504), (68509, 68520), (68528, 68607), (68681, 68735), (68787, 68799), (68851, 68857);
INSERT INTO rx_unassigned VALUES (68864, 69215), (69247, 69631), (69710, 69713), (69744, 69758), (69826, 69839), (69865, 69871), (69882, 69887), (69941, 69941), (69956, 69967), (70007, 70015), (70094, 70095), (70112, 70112);
INSERT INTO rx_unassigned VALUES (70133, 70143), (70162, 70162), (70207, 70271), (70279, 70279), (70281, 70281), (70286, 70286), (70302, 70302), (70314, 70319), (70379, 70383), (70394, 70399), (70404, 70404), (70413, 70414);
INSERT INTO rx_unassigned VALUES (70417, 70418), (70441, 70441), (70449, 70449), (70452, 70452), (70458, 70459), (70469, 70470), (70473, 70474), (70478, 70479), (70481, 70486), (70488, 70492), (70500, 70501), (70509, 70511);
INSERT INTO rx_unassigned VALUES (70517, 70655), (70746, 70746), (70748, 70748), (70750, 70783), (70856, 70863), (70874, 71039), (71094, 71095), (71134, 71167), (71237, 71247), (71258, 71263), (71277, 71295), (71352, 71359);
INSERT INTO rx_unassigned VALUES (71370, 71423), (71450, 71452), (71468, 71471), (71488, 71839), (71923, 71934), (71936, 72383), (72441, 72703), (72713, 72713), (72759, 72759), (72774, 72783), (72813, 72815), (72848, 72849);
INSERT INTO rx_unassigned VALUES (72872, 72872), (72887, 73727), (74650, 74751), (74863, 74863), (74869, 74879), (75076, 77823), (78895, 82943), (83527, 92159), (92729, 92735), (92767, 92767), (92778, 92781), (92784, 92879);
INSERT INTO rx_unassigned VALUES (92910, 92911), (92918, 92927), (92998, 93007), (93018, 93018), (93026, 93026), (93048, 93052), (93072, 93951), (94021, 94031), (94079, 94094), (94112, 94175), (94177, 94207), (100333, 100351);
INSERT INTO rx_unassigned VALUES (101107, 110591), (110594, 113663), (113771, 113775), (113789, 113791), (113801, 113807), (113818, 113819), (113828, 118783), (119030, 119039), (119079, 119080), (119273, 119295), (119366, 119551), (119639, 119647);
INSERT INTO rx_unassigned VALUES (119666, 119807), (119893, 119893), (119965, 119965), (119968, 119969), (119971, 119972), (119975, 119976), (119981, 119981), (119994, 119994), (119996, 119996), (120004, 120004), (120070, 120070), (120075, 120076);
INSERT INTO rx_unassigned VALUES (120085, 120085), (120093, 120093), (120122, 120122), (120127, 120127), (120133, 120133), (120135, 120137), (120145, 120145), (120486, 120487), (120780, 120781), (121484, 121498), (121504, 121504), (121520, 122879);
INSERT INTO rx_unassigned VALUES (122887, 122887), (122905, 122906), (122914, 122914), (122917, 122917), (122923, 124927), (125125, 125126), (125143, 125183), (125259, 125263), (125274, 125277), (125280, 126463), (126468, 126468), (126496, 126496);
INSERT INTO rx_unassigned VALUES (126499, 126499), (126501, 126502), (126504, 126504), (126515, 126515), (126520, 126520), (126522, 126522), (126524, 126529), (126531, 126534), (126536, 126536), (126538, 126538), (126540, 126540), (126544, 126544);
INSERT INTO rx_unassigned VALUES (126547, 126547), (126549, 126550), (126552, 126552), (126554, 126554), (126556, 126556), (126558, 126558), (126560, 126560), (126563, 126563), (126565, 126566), (126571, 126571), (126579, 126579), (126584, 126584);
INSERT INTO rx_unassigned VALUES (126589, 126589), (126591, 126591), (126602, 126602), (126620, 126624), (126628, 126628), (126634, 126634), (126652, 126703), (126706, 126975), (127020, 127023), (127124, 127135), (127151, 127152), (127168, 127168);
INSERT INTO rx_unassigned VALUES (127184, 127184), (127222, 127231), (127245, 127247), (127279, 127279), (127340, 127343), (127405, 127461), (127491, 127503), (127548, 127551), (127561, 127567), (127570, 127743), (128723, 128735), (128749, 128751);
INSERT INTO rx_unassigned VALUES (128759, 128767), (128884, 128895), (128981, 129023), (129036, 129039), (129096, 129103), (129114, 129119), (129160, 129167), (129198, 129295), (129311, 129311), (129320, 129327), (129329, 129330), (129343, 129343);
INSERT INTO rx_unassigned VALUES (129356, 129359), (129375, 129407), (129426, 129471), (129473, 131071), (173783, 173823), (177973, 177983), (178206, 178207), (183970, 194559), (195102, 262143), (917504, 917504), (917506, 917535), (917632, 917759);
INSERT INTO rx_unassigned VALUES (918000, 983039);
CREATE TABLE rx_points (n INT NOT NULL PRIMARY KEY, c VARCHAR(1) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL);
INSERT INTO rx_points SELECT n, CONVERT(CHAR(n USING utf32) USING utf8mb4) FROM (SELECT a.n + 16 * b.n + 256 * c.n + 4096 * d.n + 65536 * e.n AS n FROM rx_digits a, rx_digits b, rx_digits c, rx_digits d, rx_digits e) x WHERE (n < 55296 OR n > 57343) AND (n < 262144 OR n BETWEEN 917504 AND 983039);
DELETE rx_points FROM rx_unassigned JOIN rx_points ON rx_points.n BETWEEN rx_unassigned.lo AND rx_unassigned.hi;
SELECT COUNT(*) AS points FROM rx_points;
-- Under `utf8mb4_unicode_ci`.
SELECT 'W_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\p{L}\\p{Nl}\\p{M}\\p{Nd}\\p{Pc}\\x{200C}\\x{200D}\\x{24B6}-\\x{24E9}\\x{1F130}-\\x{1F149}\\x{1F150}-\\x{1F169}\\x{1F170}-\\x{1F189}])$', '^(?:\\W)$')
UNION ALL
SELECT 'w_in_set' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\p{L}\\p{Nl}\\p{M}\\p{Nd}\\p{Pc}\\x{200C}\\x{200D}\\x{24B6}-\\x{24E9}\\x{1F130}-\\x{1F149}\\x{1F150}-\\x{1F169}\\x{1F170}-\\x{1F189},])$', '^(?:[\\w,])$')
UNION ALL
SELECT 'w_in_neg_set' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\p{L}\\p{Nl}\\p{M}\\p{Nd}\\p{Pc}\\x{200C}\\x{200D}\\x{24B6}-\\x{24E9}\\x{1F130}-\\x{1F149}\\x{1F150}-\\x{1F169}\\x{1F170}-\\x{1F189},])$', '^(?:[^\\w,])$')
UNION ALL
SELECT 's_to_r' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\S\\x{180E}])$', '^(?:\\s)$')
UNION ALL
SELECT 'S_to_r' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\S\\x{180E}])$', '^(?:\\S)$')
UNION ALL
SELECT 'h_to_r' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\H\\x{180E}])$', '^(?:\\h)$')
UNION ALL
SELECT 'H_to_r' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\H\\x{180E}])$', '^(?:\\H)$')
UNION ALL
SELECT 's_to_m' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\s)$', '^(?:[\\s\\x{180E}])$')
UNION ALL
SELECT 'S_to_m' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\S)$', '^(?:[^\\s\\x{180E}])$')
UNION ALL
SELECT 'h_to_m' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\h)$', '^(?:[\\h\\x{180E}])$')
UNION ALL
SELECT 'H_to_m' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\H)$', '^(?:[^\\h\\x{180E}])$')
UNION ALL
SELECT 'd' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\d)$', '^(?:\\d)$')
UNION ALL
SELECT 'D' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\D)$', '^(?:\\D)$')
UNION ALL
SELECT 'digit' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[[:digit:]])$', '^(?:[[:digit:]])$')
UNION ALL
SELECT 'cntrl' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[[:cntrl:]])$', '^(?:[[:cntrl:]])$')
UNION ALL
SELECT 'v' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\v)$', '^(?:\\v)$')
UNION ALL
SELECT 'V' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\V)$', '^(?:\\V)$')
UNION ALL
SELECT 'p_L_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{L})$', '^(?:\\p{L})$')
UNION ALL
SELECT 'P_L_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{L})$', '^(?:\\P{L})$')
UNION ALL
SELECT 'p_Lu_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Lu})$', '^(?:\\p{Lu})$')
UNION ALL
SELECT 'P_Lu_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Lu})$', '^(?:\\P{Lu})$')
UNION ALL
SELECT 'p_Ll_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Ll})$', '^(?:\\p{Ll})$')
UNION ALL
SELECT 'P_Ll_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Ll})$', '^(?:\\P{Ll})$')
UNION ALL
SELECT 'p_Lt_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Lt})$', '^(?:\\p{Lt})$')
UNION ALL
SELECT 'P_Lt_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Lt})$', '^(?:\\P{Lt})$')
UNION ALL
SELECT 'p_Lm_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Lm})$', '^(?:\\p{Lm})$')
UNION ALL
SELECT 'P_Lm_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Lm})$', '^(?:\\P{Lm})$')
UNION ALL
SELECT 'p_Lo_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Lo})$', '^(?:\\p{Lo})$')
UNION ALL
SELECT 'P_Lo_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Lo})$', '^(?:\\P{Lo})$')
UNION ALL
SELECT 'p_M_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{M})$', '^(?:\\p{M})$')
UNION ALL
SELECT 'P_M_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{M})$', '^(?:\\P{M})$')
UNION ALL
SELECT 'p_Mn_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Mn})$', '^(?:\\p{Mn})$')
UNION ALL
SELECT 'P_Mn_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Mn})$', '^(?:\\P{Mn})$')
UNION ALL
SELECT 'p_Mc_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Mc})$', '^(?:\\p{Mc})$')
UNION ALL
SELECT 'P_Mc_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Mc})$', '^(?:\\P{Mc})$')
UNION ALL
SELECT 'p_Me_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Me})$', '^(?:\\p{Me})$')
UNION ALL
SELECT 'P_Me_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Me})$', '^(?:\\P{Me})$')
UNION ALL
SELECT 'p_N_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{N})$', '^(?:\\p{N})$')
UNION ALL
SELECT 'P_N_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{N})$', '^(?:\\P{N})$')
UNION ALL
SELECT 'p_Nd_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Nd})$', '^(?:\\p{Nd})$')
UNION ALL
SELECT 'P_Nd_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Nd})$', '^(?:\\P{Nd})$')
UNION ALL
SELECT 'p_Nl_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Nl})$', '^(?:\\p{Nl})$')
UNION ALL
SELECT 'P_Nl_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Nl})$', '^(?:\\P{Nl})$')
UNION ALL
SELECT 'p_No_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{No})$', '^(?:\\p{No})$')
UNION ALL
SELECT 'P_No_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{No})$', '^(?:\\P{No})$')
UNION ALL
SELECT 'p_P_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{P})$', '^(?:\\p{P})$')
UNION ALL
SELECT 'P_P_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{P})$', '^(?:\\P{P})$')
UNION ALL
SELECT 'p_Pc_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Pc})$', '^(?:\\p{Pc})$')
UNION ALL
SELECT 'P_Pc_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Pc})$', '^(?:\\P{Pc})$')
UNION ALL
SELECT 'p_Pd_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Pd})$', '^(?:\\p{Pd})$')
UNION ALL
SELECT 'P_Pd_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Pd})$', '^(?:\\P{Pd})$')
UNION ALL
SELECT 'p_Ps_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Ps})$', '^(?:\\p{Ps})$')
UNION ALL
SELECT 'P_Ps_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Ps})$', '^(?:\\P{Ps})$')
UNION ALL
SELECT 'p_Pe_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Pe})$', '^(?:\\p{Pe})$')
UNION ALL
SELECT 'P_Pe_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Pe})$', '^(?:\\P{Pe})$')
UNION ALL
SELECT 'p_Pi_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Pi})$', '^(?:\\p{Pi})$')
UNION ALL
SELECT 'P_Pi_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Pi})$', '^(?:\\P{Pi})$')
UNION ALL
SELECT 'p_Pf_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Pf})$', '^(?:\\p{Pf})$')
UNION ALL
SELECT 'P_Pf_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Pf})$', '^(?:\\P{Pf})$')
UNION ALL
SELECT 'p_Po_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Po})$', '^(?:\\p{Po})$')
UNION ALL
SELECT 'P_Po_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Po})$', '^(?:\\P{Po})$')
UNION ALL
SELECT 'p_S_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{S})$', '^(?:\\p{S})$')
UNION ALL
SELECT 'P_S_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{S})$', '^(?:\\P{S})$')
UNION ALL
SELECT 'p_Sm_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Sm})$', '^(?:\\p{Sm})$')
UNION ALL
SELECT 'P_Sm_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Sm})$', '^(?:\\P{Sm})$')
UNION ALL
SELECT 'p_Sc_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Sc})$', '^(?:\\p{Sc})$')
UNION ALL
SELECT 'P_Sc_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Sc})$', '^(?:\\P{Sc})$')
UNION ALL
SELECT 'p_Sk_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Sk})$', '^(?:\\p{Sk})$')
UNION ALL
SELECT 'P_Sk_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Sk})$', '^(?:\\P{Sk})$')
UNION ALL
SELECT 'p_So_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{So})$', '^(?:\\p{So})$')
UNION ALL
SELECT 'P_So_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{So})$', '^(?:\\P{So})$')
UNION ALL
SELECT 'p_Z_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Z})$', '^(?:\\p{Z})$')
UNION ALL
SELECT 'P_Z_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Z})$', '^(?:\\P{Z})$')
UNION ALL
SELECT 'p_Zs_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Zs})$', '^(?:\\p{Zs})$')
UNION ALL
SELECT 'P_Zs_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Zs})$', '^(?:\\P{Zs})$')
UNION ALL
SELECT 'p_Zl_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Zl})$', '^(?:\\p{Zl})$')
UNION ALL
SELECT 'P_Zl_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Zl})$', '^(?:\\P{Zl})$')
UNION ALL
SELECT 'p_Zp_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Zp})$', '^(?:\\p{Zp})$')
UNION ALL
SELECT 'P_Zp_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Zp})$', '^(?:\\P{Zp})$')
UNION ALL
SELECT 'p_C_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{C})$', '^(?:\\p{C})$')
UNION ALL
SELECT 'P_C_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{C})$', '^(?:\\P{C})$')
UNION ALL
SELECT 'p_Cc_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Cc})$', '^(?:\\p{Cc})$')
UNION ALL
SELECT 'P_Cc_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Cc})$', '^(?:\\P{Cc})$')
UNION ALL
SELECT 'p_Cf_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Cf})$', '^(?:\\p{Cf})$')
UNION ALL
SELECT 'P_Cf_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Cf})$', '^(?:\\P{Cf})$')
UNION ALL
SELECT 'p_Co_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Co})$', '^(?:\\p{Co})$')
UNION ALL
SELECT 'P_Co_ci' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Co})$', '^(?:\\P{Co})$')
UNION ALL
SELECT 'range_az' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[a-z])$', '^(?:[a-z])$')
UNION ALL
SELECT 'negrange_az' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^a-z])$', '^(?:[^a-z])$')
UNION ALL
SELECT 'range_AZ' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[A-Z])$', '^(?:[A-Z])$')
UNION ALL
SELECT 'negrange_AZ' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^A-Z])$', '^(?:[^A-Z])$')
UNION ALL
SELECT 'range_latin1' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\x{C0}-\\x{FF}])$', '^(?:[\\x{C0}-\\x{FF}])$')
UNION ALL
SELECT 'negrange_latin1' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\x{C0}-\\x{FF}])$', '^(?:[^\\x{C0}-\\x{FF}])$')
UNION ALL
SELECT 'range_ascii_latin' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\x{0}-\\x{17F}])$', '^(?:[\\x{0}-\\x{17F}])$')
UNION ALL
SELECT 'negrange_ascii_latin' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\x{0}-\\x{17F}])$', '^(?:[^\\x{0}-\\x{17F}])$')
UNION ALL
SELECT 'range_all' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\x{0}-\\x{10FFFF}])$', '^(?:[\\x{0}-\\x{10FFFF}])$')
UNION ALL
SELECT 'negrange_all' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\x{0}-\\x{10FFFF}])$', '^(?:[^\\x{0}-\\x{10FFFF}])$')
UNION ALL
SELECT 's_list' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\x{9}-\\x{D}\\x{20}\\x{85}\\x{A0}\\x{1680}\\x{2000}-\\x{200A}\\x{2028}\\x{2029}\\x{202F}\\x{205F}\\x{3000}])$', '^(?:\\s)$')
UNION ALL
SELECT 's_list_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\x{9}-\\x{D}\\x{20}\\x{85}\\x{A0}\\x{1680}\\x{2000}-\\x{200A}\\x{2028}\\x{2029}\\x{202F}\\x{205F}\\x{3000}])$', '^(?:[^\\s])$')
UNION ALL
SELECT 'space_list' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\x{9}-\\x{D}\\x{20}\\x{85}\\x{A0}\\x{1680}\\x{2000}-\\x{200A}\\x{2028}\\x{2029}\\x{202F}\\x{205F}\\x{3000}])$', '^(?:[[:space:]])$')
UNION ALL
SELECT 'space_list_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\x{9}-\\x{D}\\x{20}\\x{85}\\x{A0}\\x{1680}\\x{2000}-\\x{200A}\\x{2028}\\x{2029}\\x{202F}\\x{205F}\\x{3000}a])$', '^(?:[^[:space:]a])$')
UNION ALL
SELECT 'h_list' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\x{9}\\x{20}\\x{A0}\\x{1680}\\x{2000}-\\x{200A}\\x{202F}\\x{205F}\\x{3000}])$', '^(?:\\h)$')
UNION ALL
SELECT 'h_list_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\x{9}\\x{20}\\x{A0}\\x{1680}\\x{2000}-\\x{200A}\\x{202F}\\x{205F}\\x{3000}])$', '^(?:[^\\h])$')
UNION ALL
SELECT 'blank_list' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\x{9}\\x{20}\\x{A0}\\x{1680}\\x{2000}-\\x{200A}\\x{202F}\\x{205F}\\x{3000}])$', '^(?:[[:blank:]])$')
UNION ALL
SELECT 'blank_list_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\x{9}\\x{20}\\x{A0}\\x{1680}\\x{2000}-\\x{200A}\\x{202F}\\x{205F}\\x{3000}a])$', '^(?:[^[:blank:]a])$')
UNION ALL
SELECT 'S_set' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\S\\x{180E}\\n])$', '^(?:[\\S\\n])$')
UNION ALL
SELECT 'S_set_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\S\\x{180E}\\n])$', '^(?:[^\\S\\n])$')
UNION ALL
SELECT 'H_set' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\H\\x{180E}\\t])$', '^(?:[\\H\\t])$')
UNION ALL
SELECT 'H_set_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\H\\x{180E}\\t])$', '^(?:[^\\H\\t])$')
UNION ALL
SELECT 'W_set' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:(?:[^\\p{L}\\p{Nl}\\p{M}\\p{Nd}\\p{Pc}\\x{200C}\\x{200D}\\x{24B6}-\\x{24E9}\\x{1F130}-\\x{1F149}\\x{1F150}-\\x{1F169}\\x{1F170}-\\x{1F189}]|[_]))$', '^(?:[\\W_])$')
UNION ALL
SELECT 'W_set_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:(?:(?![_])[\\p{L}\\p{Nl}\\p{M}\\p{Nd}\\p{Pc}\\x{200C}\\x{200D}\\x{24B6}-\\x{24E9}\\x{1F130}-\\x{1F149}\\x{1F150}-\\x{1F169}\\x{1F170}-\\x{1F189}]))$', '^(?:[^\\W_])$')
UNION ALL
SELECT 'm_s_set' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\sa])$', '^(?:[\\s\\x{180E}a])$')
UNION ALL
SELECT 'm_s_set_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\sa])$', '^(?:[^\\s\\x{180E}a])$')
UNION ALL
SELECT 'm_space_set' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[[:space:]a])$', '^(?:[[:space:]\\x{180E}a])$')
UNION ALL
SELECT 'm_space_set_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^[:space:]a])$', '^(?:[^[:space:]\\x{180E}a])$')
UNION ALL
SELECT 'm_blank_set' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[[:blank:]a])$', '^(?:[[:blank:]\\x{180E}a])$')
UNION ALL
SELECT 'm_blank_set_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^[:blank:]a])$', '^(?:[^[:blank:]\\x{180E}a])$')
UNION ALL
SELECT 'm_h_set' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\ha])$', '^(?:[\\h\\x{180E}a])$')
UNION ALL
SELECT 'm_h_set_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\ha])$', '^(?:[^\\h\\x{180E}a])$')
UNION ALL
SELECT 'm_S_set' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\S\\n])$', '^(?:(?:[^\\s\\x{180E}]|[\\n]))$')
UNION ALL
SELECT 'm_S_set_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\S\\n])$', '^(?:(?![\\n])[\\s\\x{180E}])$')
UNION ALL
SELECT 'm_H_set' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\H\\t])$', '^(?:(?:[^\\h\\x{180E}]|[\\t]))$')
UNION ALL
SELECT 'm_H_set_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\H\\t])$', '^(?:(?![\\t])[\\h\\x{180E}])$')
UNION ALL
SELECT 'P_Lo' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Lo})$', '^(?:\\P{Lo})$')
UNION ALL
SELECT 'P_Lm' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Lm})$', '^(?:\\P{Lm})$')
UNION ALL
SELECT 'P_N' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{N})$', '^(?:\\P{N})$')
UNION ALL
SELECT 'P_Nl' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Nl})$', '^(?:\\P{Nl})$')
UNION ALL
SELECT 'P_So' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{So})$', '^(?:\\P{So})$')
UNION ALL
SELECT 'P_S' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{S})$', '^(?:\\P{S})$')
UNION ALL
SELECT 'P_Cf' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Cf})$', '^(?:\\P{Cf})$')
UNION ALL
SELECT 'P_Mc' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Mc})$', '^(?:\\P{Mc})$')
UNION ALL
SELECT 'P_Me' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Me})$', '^(?:\\P{Me})$')
UNION ALL
SELECT 'p_props_set' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\p{Nd}\\p{Sm}x])$', '^(?:[\\p{Nd}\\p{Sm}x])$')
UNION ALL
SELECT 'p_props_set_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\p{Nd}\\P{Zs}x])$', '^(?:[^\\p{Nd}\\P{Zs}x])$')
UNION ALL
SELECT 'r_greek' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[α-ω])$', '^(?:[α-ω])$')
UNION ALL
SELECT 'r_greek_upper' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[Α-Ω])$', '^(?:[Α-Ω])$')
UNION ALL
SELECT 'r_greek_ext' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\x{1F00}-\\x{1FFF}])$', '^(?:[\\x{1F00}-\\x{1FFF}])$')
UNION ALL
SELECT 'r_cyrillic' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\x{400}-\\x{4FF}])$', '^(?:[\\x{400}-\\x{4FF}])$')
UNION ALL
SELECT 'r_mixed_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^a-fK-Z\\x{100}-\\x{17F}])$', '^(?:[^a-fK-Z\\x{100}-\\x{17F}])$')
UNION ALL
SELECT 'r_ligatures' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\x{FB00}-\\x{FB06}])$', '^(?:[\\x{FB00}-\\x{FB06}])$')
UNION ALL
SELECT 'd_digit_set' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\d[:cntrl:]\\v])$', '^(?:[\\d[:cntrl:]\\v])$')
UNION ALL
SELECT 'd_digit_set_neg' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c COLLATE utf8mb4_unicode_ci REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\D\\V])$', '^(?:[^\\D\\V])$');
-- Under `utf8mb4_bin`.
SELECT 'w_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[\\p{L}\\p{Nl}\\p{M}\\p{Nd}\\p{Pc}\\x{200C}\\x{200D}\\x{24B6}-\\x{24E9}\\x{1F130}-\\x{1F149}\\x{1F150}-\\x{1F169}\\x{1F170}-\\x{1F189}])$', '^(?:\\w)$')
UNION ALL
SELECT 'p_L_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{L})$', '^(?:\\p{L})$')
UNION ALL
SELECT 'p_Lu_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Lu})$', '^(?:\\p{Lu})$')
UNION ALL
SELECT 'p_Ll_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Ll})$', '^(?:\\p{Ll})$')
UNION ALL
SELECT 'p_Lt_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Lt})$', '^(?:\\p{Lt})$')
UNION ALL
SELECT 'p_Lm_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Lm})$', '^(?:\\p{Lm})$')
UNION ALL
SELECT 'p_Lo_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Lo})$', '^(?:\\p{Lo})$')
UNION ALL
SELECT 'p_M_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{M})$', '^(?:\\p{M})$')
UNION ALL
SELECT 'p_Mn_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Mn})$', '^(?:\\p{Mn})$')
UNION ALL
SELECT 'p_Mc_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Mc})$', '^(?:\\p{Mc})$')
UNION ALL
SELECT 'p_Me_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Me})$', '^(?:\\p{Me})$')
UNION ALL
SELECT 'p_N_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{N})$', '^(?:\\p{N})$')
UNION ALL
SELECT 'p_Nd_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Nd})$', '^(?:\\p{Nd})$')
UNION ALL
SELECT 'p_Nl_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Nl})$', '^(?:\\p{Nl})$')
UNION ALL
SELECT 'p_No_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{No})$', '^(?:\\p{No})$')
UNION ALL
SELECT 'p_P_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{P})$', '^(?:\\p{P})$')
UNION ALL
SELECT 'p_Pc_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Pc})$', '^(?:\\p{Pc})$')
UNION ALL
SELECT 'p_Pd_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Pd})$', '^(?:\\p{Pd})$')
UNION ALL
SELECT 'p_Ps_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Ps})$', '^(?:\\p{Ps})$')
UNION ALL
SELECT 'p_Pe_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Pe})$', '^(?:\\p{Pe})$')
UNION ALL
SELECT 'p_Pi_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Pi})$', '^(?:\\p{Pi})$')
UNION ALL
SELECT 'p_Pf_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Pf})$', '^(?:\\p{Pf})$')
UNION ALL
SELECT 'p_Po_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Po})$', '^(?:\\p{Po})$')
UNION ALL
SELECT 'p_S_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{S})$', '^(?:\\p{S})$')
UNION ALL
SELECT 'p_Sm_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Sm})$', '^(?:\\p{Sm})$')
UNION ALL
SELECT 'p_Sc_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Sc})$', '^(?:\\p{Sc})$')
UNION ALL
SELECT 'p_Sk_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Sk})$', '^(?:\\p{Sk})$')
UNION ALL
SELECT 'p_So_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{So})$', '^(?:\\p{So})$')
UNION ALL
SELECT 'p_Z_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Z})$', '^(?:\\p{Z})$')
UNION ALL
SELECT 'p_Zs_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Zs})$', '^(?:\\p{Zs})$')
UNION ALL
SELECT 'p_Zl_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Zl})$', '^(?:\\p{Zl})$')
UNION ALL
SELECT 'p_Zp_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Zp})$', '^(?:\\p{Zp})$')
UNION ALL
SELECT 'p_C_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{C})$', '^(?:\\p{C})$')
UNION ALL
SELECT 'p_Cc_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Cc})$', '^(?:\\p{Cc})$')
UNION ALL
SELECT 'p_Cf_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Cf})$', '^(?:\\p{Cf})$')
UNION ALL
SELECT 'p_Co_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\p{Co})$', '^(?:\\p{Co})$')
UNION ALL
SELECT 'W_set_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:(?:[^\\p{L}\\p{Nl}\\p{M}\\p{Nd}\\p{Pc}\\x{200C}\\x{200D}\\x{24B6}-\\x{24E9}\\x{1F130}-\\x{1F149}\\x{1F150}-\\x{1F169}\\x{1F170}-\\x{1F189}]|[_]))$', '^(?:[\\W_])$')
UNION ALL
SELECT 'W_set_neg_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:(?:(?![_])[\\p{L}\\p{Nl}\\p{M}\\p{Nd}\\p{Pc}\\x{200C}\\x{200D}\\x{24B6}-\\x{24E9}\\x{1F130}-\\x{1F149}\\x{1F150}-\\x{1F169}\\x{1F170}-\\x{1F189}]))$', '^(?:[^\\W_])$')
UNION ALL
SELECT 'W_alone_neg_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:[^\\p{L}\\p{Nl}\\p{M}\\p{Nd}\\p{Pc}\\x{200C}\\x{200D}\\x{24B6}-\\x{24E9}\\x{1F130}-\\x{1F149}\\x{1F150}-\\x{1F169}\\x{1F170}-\\x{1F189}])$', '^(?:\\W)$')
UNION ALL
SELECT 'P_L_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{L})$', '^(?:\\P{L})$')
UNION ALL
SELECT 'P_Lu_cs' AS form, COUNT(*) AS points, LEFT(SHA2(IFNULL(GROUP_CONCAT(n ORDER BY n), ''), 256), 16) AS hash FROM rx_points WHERE c REGEXP IF(VERSION() LIKE '%MariaDB%', '^(?:\\P{Lu})$', '^(?:\\P{Lu})$');
DROP TABLE rx_points, rx_unassigned, rx_digits;
