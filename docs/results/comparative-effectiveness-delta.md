Comparison Matrix:
delta = mean_outcome_exposed - mean_outcome_control (positive = exposed arm has higher average outcome)
n_exposed = patients assigned to the exposed arm (have the exposed_medication_code, not the control)
n_control = patients assigned to the control arm (have the control_medication_code, not the exposed)
mean_outcome_exposed = average value of outcome_observation_code in the exposed arm
mean_outcome_control = average value of outcome_observation_code in the control arm

Not writing raw and released, as raw can be seen in Raw section for convenience
For the DP stuff, Ill just write delta%, as this is the main result, and other stuff is maybe too obscured through DP.
also just using one dp seed now (42), as otherwise it might be too time consuming.

Coarsening active:

    Amount of patient files: 100 (doesnt include delta% as DP inconclusive anyways)

        Raw:

            1:
                parameters: {   "exposed_medication_code": "308136",   "control_medication_code": "106892",   "outcome_observation_code": "8867-4",   "min_age": 20 }

                raw:
                    delta: 0.9612478683250032
                    mean_outcome_control: 79.0298166228
                    mean_outcome_exposed: 79.991064491125
                    n_control: 5
                    n_exposed: 8

                inconclusive due to too small cohort size

            2:
                parameters: {   "exposed_medication_code": "314076",   "control_medication_code": "310798",   "outcome_observation_code": "39156-5",   "min_age": 20 }

                raw:
                    delta: 0.18276605531110945
                    mean_outcome_control: 28.4313181818
                    mean_outcome_exposed: 28.614084237111108
                    n_control: 5
                    n_exposed: 18

                inconclusive due to too small cohort size

            3:
                parameters: {   "exposed_medication_code": "860975",   "control_medication_code": "856987",   "outcome_observation_code": "72514-3" }

                raw:
                    delta: 0.5539527782424236
                    mean_outcome_control: 2.4891882720909093
                    mean_outcome_exposed: 3.043141050333333
                    n_control: 11
                    n_exposed: 3

                inconclusive due to too small cohort size

            4:
                parameters: {   "exposed_medication_code": "106892",   "control_medication_code": "314076",   "outcome_observation_code": "29463-7",   "min_age": 20,   "max_age": 84,   "gender": "female",   "condition_codes": [     "314529007"   ] }

                raw:
                    delta: 4.126117336000007
                    mean_outcome_control: 75.89085297433333
                    mean_outcome_exposed: 80.01697031033333
                    n_control: 3
                    n_exposed: 3

                inconclusive due to too small cohort size

            5:
                parameters: {   "exposed_medication_code": "310798",   "control_medication_code": "308136",   "outcome_observation_code": "9279-1",   "min_age": 40,   "gender": "male",   "condition_codes": [     "160903007",     "160904001"   ] }

                raw:
                    delta: 0.13811410829999993
                    mean_outcome_control: 13.9545454545
                    mean_outcome_exposed: 14.0926595628
                    n_control: 2
                    n_exposed: 5

                inconclusive due to too small cohort size

            6:
                parameters: {   "exposed_medication_code": "1664463",   "control_medication_code": "314231",   "outcome_observation_code": "93025-5",   "min_age": 30,   "max_age": 79,   "condition_codes": [     "73595000"   ] }

                raw:
                    delta: null
                    mean_outcome_control: null
                    mean_outcome_exposed: null
                    n_control: 0
                    n_exposed: 0

                inconclusive due to too small cohort size

            7:
                parameters: {   "exposed_medication_code": "351137",   "control_medication_code": "349094",   "outcome_observation_code": "2947-0",   "min_age": 20,   "condition_codes": [     "314529007"   ] }

                raw:
                    delta: null
                    mean_outcome_control: null
                    mean_outcome_exposed: 139.953
                    n_control: 0
                    n_exposed: 1

                inconclusive due to too small cohort size

            8:
                parameters: {   "exposed_medication_code": "206905",   "control_medication_code": "897685",   "outcome_observation_code": "6299-2",   "gender": "female" }

                raw:
                    delta: null
                    mean_outcome_control: 14.996
                    mean_outcome_exposed: null
                    n_control: 1
                    n_exposed: 0

                inconclusive due to too small cohort size

            9:
                parameters: {   "exposed_medication_code": "313782",   "control_medication_code": "313988",   "outcome_observation_code": "4548-4" }

                raw:
                    delta: 0.09629309989999957
                    mean_outcome_control: 5.399375
                    mean_outcome_exposed: 5.4956680999
                    n_control: 1
                    n_exposed: 20

                inconclusive due to too small cohort size

            10:
                parameters: {   "exposed_medication_code": "904419",   "control_medication_code": "200033",   "outcome_observation_code": "85354-9",   "min_age": 20,   "condition_codes": [     "66383009",     "741062008"   ] }

                raw:
                    delta: null
                    mean_outcome_control: null
                    mean_outcome_exposed: null
                    n_control: 0
                    n_exposed: 0

                inconclusive due to too small cohort size
        
        DP (Seed 42):

            1:
                parameters: {   "exposed_medication_code": "308136",   "control_medication_code": "106892",   "outcome_observation_code": "8867-4",   "min_age": 20 }

                inconclusive due to too small cohort size

            2:
                parameters: {   "exposed_medication_code": "314076",   "control_medication_code": "310798",   "outcome_observation_code": "39156-5",   "min_age": 20 }

                inconclusive due to too small cohort size

            3:
                parameters: {   "exposed_medication_code": "860975",   "control_medication_code": "856987",   "outcome_observation_code": "72514-3" }

                inconclusive due to too small cohort size

            4:
                parameters: {   "exposed_medication_code": "106892",   "control_medication_code": "314076",   "outcome_observation_code": "29463-7",   "min_age": 20,   "max_age": 84,   "gender": "female",   "condition_codes": [     "314529007"   ] }

                inconclusive due to too small cohort size

            5:
                parameters: {   "exposed_medication_code": "310798",   "control_medication_code": "308136",   "outcome_observation_code": "9279-1",   "min_age": 40,   "gender": "male",   "condition_codes": [     "160903007",     "160904001"   ] }

                inconclusive due to too small cohort size

            6:
                parameters: {   "exposed_medication_code": "1664463",   "control_medication_code": "314231",   "outcome_observation_code": "93025-5",   "min_age": 30,   "max_age": 79,   "condition_codes": [     "73595000"   ] }

                inconclusive due to too small cohort size

            7:
                parameters: {   "exposed_medication_code": "351137",   "control_medication_code": "349094",   "outcome_observation_code": "2947-0",   "min_age": 20,   "condition_codes": [     "314529007"   ] }

                inconclusive due to too small cohort size

            8:
                parameters: {   "exposed_medication_code": "206905",   "control_medication_code": "897685",   "outcome_observation_code": "6299-2",   "gender": "female" }

                inconclusive due to too small cohort size

            9:
                parameters: {   "exposed_medication_code": "313782",   "control_medication_code": "313988",   "outcome_observation_code": "4548-4" }

                inconclusive due to too small cohort size

            10:
                parameters: {   "exposed_medication_code": "904419",   "control_medication_code": "200033",   "outcome_observation_code": "85354-9",   "min_age": 20,   "condition_codes": [     "66383009",     "741062008"   ] }

                inconclusive due to too small cohort size

    Amount of patient files: 500

        Raw: 

            1:
                parameters: {   "exposed_medication_code": "308136",   "control_medication_code": "106892",   "outcome_observation_code": "8867-4",   "min_age": 20 }
                
                raw:
                    delta: -0.36991053538223184
                    delta%: -0.46479276034355754
                    mean_outcome_control: 79.58612244924139
                    mean_outcome_exposed: 79.21621191385915
                    n_control: 29
                    n_exposed: 71

                Preserved

            2:
                parameters: {   "exposed_medication_code": "314076",   "control_medication_code": "310798",   "outcome_observation_code": "39156-5",   "min_age": 20 }

                raw:
                    delta: -0.2974378377052673
                    delta%: -1.0291364736559672
                    mean_outcome_control: 28.901690428736845
                    mean_outcome_exposed: 28.604252591031578
                    n_control: 38
                    n_exposed: 95

                Preserved

            3:
                parameters: {   "exposed_medication_code": "860975",   "control_medication_code": "856987",   "outcome_observation_code": "72514-3" }

                raw:
                    delta: -0.05934910234219881
                    delta%: -2.640087892838583
                    mean_outcome_control: 2.247997216425532
                    mean_outcome_exposed: 2.188648114083333
                    n_control: 47
                    n_exposed: 24

                inconclusive due to too small cohort size

            4:
                parameters: {   "exposed_medication_code": "106892",   "control_medication_code": "314076",   "outcome_observation_code": "29463-7",   "min_age": 20,   "max_age": 84,   "gender": "female",   "condition_codes": [     "314529007"   ] }

                raw:
                    delta: -0.8244031685636344
                    delta%: -1.0785042580334525
                    mean_outcome_control: 76.43949130686363
                    mean_outcome_exposed: 75.6150881383
                    n_control: 22
                    n_exposed: 10

                inconclusive due to too small cohort size

            5:
                parameters: {   "exposed_medication_code": "310798",   "control_medication_code": "308136",   "outcome_observation_code": "9279-1",   "min_age": 40,   "gender": "male",   "condition_codes": [     "160903007",     "160904001"   ] }

                raw:
                    delta: -0.042553943719999765
                    delta%: -0.3037386578806256
                    mean_outcome_control: 14.0100519364
                    mean_outcome_exposed: 13.96749799268
                    n_control: 15
                    n_exposed: 50

                inconclusive due to too small cohort size

            6:
                parameters: {   "exposed_medication_code": "1664463",   "control_medication_code": "314231",   "outcome_observation_code": "93025-5",   "min_age": 30,   "max_age": 79,   "condition_codes": [     "73595000"   ] }

                raw:
                    delta: null
                    delta%: null
                    mean_outcome_control: null
                    mean_outcome_exposed: null
                    n_control: 0
                    n_exposed: 0

                inconclusive due to too small cohort size

            7:
                parameters: {   "exposed_medication_code": "351137",   "control_medication_code": "349094",   "outcome_observation_code": "2947-0",   "min_age": 20,   "condition_codes": [     "314529007"   ] }

                raw:
                    delta: null
                    delta%: null
                    mean_outcome_control: null
                    mean_outcome_exposed: 140.464166667
                    n_control: 0
                    n_exposed: 1

                inconclusive due to too small cohort size

            8:
                parameters: {   "exposed_medication_code": "206905",   "control_medication_code": "897685",   "outcome_observation_code": "6299-2",   "gender": "female" }

                raw:
                    delta: -0.5325739858333343
                    delta%: -3.714236021785833
                    mean_outcome_control: 14.338722222
                    mean_outcome_exposed: 13.806148236166665
                    n_control: 2
                    n_exposed: 6

                inconclusive due to too small cohort size

            9:
                parameters: {   "exposed_medication_code": "313782",   "control_medication_code": "313988",   "outcome_observation_code": "4548-4" }

                raw:
                    delta: 0.593046396070295
                    delta%: 11.805017249238748
                    mean_outcome_control: 5.023680893888889
                    mean_outcome_exposed: 5.616727289959184
                    n_control: 9
                    n_exposed: 98

                Preserved

            10:
                parameters: {   "exposed_medication_code": "904419",   "control_medication_code": "200033",   "outcome_observation_code": "85354-9",   "min_age": 20,   "condition_codes": [     "66383009",     "741062008"   ] }

                raw:
                    delta: null
                    delta%: null
                    mean_outcome_control: null
                    mean_outcome_exposed: null
                    n_control: 0
                    n_exposed: 0

                inconclusive due to too small cohort size

        DP (Seed 42): 

            1:
                parameters: {   "exposed_medication_code": "308136",   "control_medication_code": "106892",   "outcome_observation_code": "8867-4",   "min_age": 20 }
                
                released: (Epsilon: 0.5 | 2.5)
                    delta%: 2.750856731323311 | 0.1783371379898161

                Preserved (3%p | 0.65%p)

            2:
                parameters: {   "exposed_medication_code": "314076",   "control_medication_code": "310798",   "outcome_observation_code": "39156-5",   "min_age": 20 }

                released: (Epsilon: 0.5 | 2.5)
                    delta%: 1.3886450990258892 | -0.5455801591195959

                Preserved (2.4%p | 0.5%p)

            3:
                parameters: {   "exposed_medication_code": "860975",   "control_medication_code": "856987",   "outcome_observation_code": "72514-3" }

                inconclusive due to too small cohort size

            4:
                parameters: {   "exposed_medication_code": "106892",   "control_medication_code": "314076",   "outcome_observation_code": "29463-7",   "min_age": 20,   "max_age": 84,   "gender": "female",   "condition_codes": [     "314529007"   ] }

                inconclusive due to too small cohort size

            5:
                parameters: {   "exposed_medication_code": "310798",   "control_medication_code": "308136",   "outcome_observation_code": "9279-1",   "min_age": 40,   "gender": "male",   "condition_codes": [     "160903007",     "160904001"   ] }

                inconclusive due to too small cohort size

            6:
                parameters: {   "exposed_medication_code": "1664463",   "control_medication_code": "314231",   "outcome_observation_code": "93025-5",   "min_age": 30,   "max_age": 79,   "condition_codes": [     "73595000"   ] }

                inconclusive due to too small cohort size

            7:
                parameters: {   "exposed_medication_code": "351137",   "control_medication_code": "349094",   "outcome_observation_code": "2947-0",   "min_age": 20,   "condition_codes": [     "314529007"   ] }

                inconclusive due to too small cohort size

            8:
                parameters: {   "exposed_medication_code": "206905",   "control_medication_code": "897685",   "outcome_observation_code": "6299-2",   "gender": "female" }

                inconclusive due to too small cohort size

            9:
                parameters: {   "exposed_medication_code": "313782",   "control_medication_code": "313988",   "outcome_observation_code": "4548-4" }

                released: (Epsilon: 0.5 | 2.5)
                    delta%: 14.81029714799283 | 12.406073228989564

                Preserved (3%p | 0.6%p)

            10:
                parameters: {   "exposed_medication_code": "904419",   "control_medication_code": "200033",   "outcome_observation_code": "85354-9",   "min_age": 20,   "condition_codes": [     "66383009",     "741062008"   ] }

                inconclusive due to too small cohort size

    Amount of patient files: 2888

        Raw: 

            1:
                parameters: {   "exposed_medication_code": "308136",   "control_medication_code": "106892",   "outcome_observation_code": "8867-4",   "min_age": 20 }
                
                raw:
                    delta: -0.011342694033075418
                    delta%: -0.014090277747536017
                    mean_outcome_control: 80.500145109312
                    mean_outcome_exposed: 80.48880241527893
                    n_control: 125
                    n_exposed: 337

                Preserved

            2:
                parameters: {   "exposed_medication_code": "314076",   "control_medication_code": "310798",   "outcome_observation_code": "39156-5",   "min_age": 20 }

                raw:
                    delta: 0.2141421601601543
                    delta%: 0.7489648288839357
                    mean_outcome_control: 28.591751161300753
                    mean_outcome_exposed: 28.805893321460907
                    n_control: 133
                    n_exposed: 486

                Preserved

            3:
                parameters: {   "exposed_medication_code": "860975",   "control_medication_code": "856987",   "outcome_observation_code": "72514-3" }

                raw:
                    delta: -0.14539466051334715
                    delta%: -6.319584832864389
                    mean_outcome_control: 2.3006995611046515
                    mean_outcome_exposed: 2.1553049005913043
                    n_control: 258                                                                   │
                    n_exposed: 115

                Preserved

            4:
                parameters: {   "exposed_medication_code": "106892",   "control_medication_code": "314076",   "outcome_observation_code": "29463-7",   "min_age": 20,   "max_age": 84,   "gender": "female",   "condition_codes": [     "314529007"   ] }

                raw:
                    delta: 0.24640851023585242
                    delta%: 0.32109943479741165
                    mean_outcome_control: 76.73900466106923
                    mean_outcome_exposed: 76.98541317130508
                    n_control: 130
                    n_exposed: 59

                Preserved

            5:
                parameters: {   "exposed_medication_code": "310798",   "control_medication_code": "308136",   "outcome_observation_code": "9279-1",   "min_age": 40,   "gender": "male",   "condition_codes": [     "160903007",     "160904001"   ] }

                raw:
                    delta: -0.0829896665402341
                    delta%: -0.5819941461669975
                    mean_outcome_control: 14.259536300631579
                    mean_outcome_exposed: 14.176546634091345
                    n_control: 76
                    n_exposed: 208

                Preserved

            6:
                parameters: {   "exposed_medication_code": "1664463",   "control_medication_code": "314231",   "outcome_observation_code": "93025-5",   "min_age": 30,   "max_age": 79,   "condition_codes": [     "73595000"   ] }

                raw:
                    delta: null
                    delta%: null
                    mean_outcome_control: null
                    mean_outcome_exposed: null
                    n_control: 0
                    n_exposed: 0

                inconclusive due to too small cohort size

            7:
                parameters: {   "exposed_medication_code": "351137",   "control_medication_code": "349094",   "outcome_observation_code": "2947-0",   "min_age": 20,   "condition_codes": [     "314529007"   ] }

                raw:
                    delta: 0.7315910678333353
                    delta%: 0.5246159713812704
                    mean_outcome_control: 139.45268686866666
                    mean_outcome_exposed: 140.1842779365
                    n_control: 3
                    n_exposed: 10

                inconclusive due to too small cohort size

            8:
                parameters: {   "exposed_medication_code": "206905",   "control_medication_code": "897685",   "outcome_observation_code": "6299-2",   "gender": "female" }

                raw:
                    delta: -0.07213056458571465
                    delta%: -0.522880962445682
                    mean_outcome_control: 13.7948347265
                    mean_outcome_exposed: 13.722704161914285
                    n_control: 8
                    n_exposed: 35

                inconclusive due to too small cohort size

            9:
                parameters: {   "exposed_medication_code": "313782",   "control_medication_code": "313988",   "outcome_observation_code": "4548-4" }

                raw:
                    delta: 0.18264577348040945
                    delta%: 3.3124326813615212
                    mean_outcome_control: 5.513946728883721
                    mean_outcome_exposed: 5.696592502364131
                    n_control: 43
                    n_exposed: 552

                Preserved

            10:
                parameters: {   "exposed_medication_code": "904419",   "control_medication_code": "200033",   "outcome_observation_code": "85354-9",   "min_age": 20,   "condition_codes": [     "66383009",     "741062008"   ] }

                raw:
                    delta: null
                    delta%: null
                    mean_outcome_control: null
                    mean_outcome_exposed: null
                    n_control: 0
                    n_exposed: 0

                inconclusive due to too small cohort size

        DP (Seed 42): 

            1:
                parameters: {   "exposed_medication_code": "308136",   "control_medication_code": "106892",   "outcome_observation_code": "8867-4",   "min_age": 20 }
                
                released: (Epsilon: 0.5 | 2.5)
                    delta%: 0.6819377507517862 | 0.1251153279523284

                Preserved 

            2:
                parameters: {   "exposed_medication_code": "314076",   "control_medication_code": "310798",   "outcome_observation_code": "39156-5",   "min_age": 20 }

                released: (Epsilon: 0.5 | 2.5)
                    delta%: 1.2684558614633974 | 0.852863035399828

                Preserved 

            3:
                parameters: {   "exposed_medication_code": "860975",   "control_medication_code": "856987",   "outcome_observation_code": "72514-3" }

                released: (Epsilon: 0.5 | 2.5)
                    delta%: -5.457480411505979 | -6.1471639485927065

                Preserved 

            4:
                parameters: {   "exposed_medication_code": "106892",   "control_medication_code": "314076",   "outcome_observation_code": "29463-7",   "min_age": 20,   "max_age": 84,   "gender": "female",   "condition_codes": [     "314529007"   ] }

                released: (Epsilon: 0.5 | 2.5)
                    delta%: 1.9075234084727517 | 0.5606545114395082

                Preserved  (1.6%p)

            5:
                parameters: {   "exposed_medication_code": "310798",   "control_medication_code": "308136",   "outcome_observation_code": "9279-1",   "min_age": 40,   "gender": "male",   "condition_codes": [     "160903007",     "160904001"   ] }

                released: (Epsilon: 0.5 | 2.5)
                    delta%: 0.5808888612086653 | -0.3345985385541443

                Preserved (1%p)

            6:
                parameters: {   "exposed_medication_code": "1664463",   "control_medication_code": "314231",   "outcome_observation_code": "93025-5",   "min_age": 30,   "max_age": 79,   "condition_codes": [     "73595000"   ] }

                inconclusive due to too small cohort size

            7:
                parameters: {   "exposed_medication_code": "351137",   "control_medication_code": "349094",   "outcome_observation_code": "2947-0",   "min_age": 20,   "condition_codes": [     "314529007"   ] }

                inconclusive due to too small cohort size

            8:
                parameters: {   "exposed_medication_code": "206905",   "control_medication_code": "897685",   "outcome_observation_code": "6299-2",   "gender": "female" }

                inconclusive due to too small cohort size

            9:
                parameters: {   "exposed_medication_code": "313782",   "control_medication_code": "313988",   "outcome_observation_code": "4548-4" }

                released: (Epsilon: 0.5 | 2.5)
                    delta%: 3.852877974078642 | 3.4205217399049452

                Preserved (0.5%p)

            10:
                parameters: {   "exposed_medication_code": "904419",   "control_medication_code": "200033",   "outcome_observation_code": "85354-9",   "min_age": 20,   "condition_codes": [     "66383009",     "741062008"   ] }

                inconclusive due to too small cohort size