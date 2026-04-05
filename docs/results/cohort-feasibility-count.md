Comparison Matrix:
count = patients who match the full cohort definition
population_in_scope = patients in the broader comparison pool, using the same age/gender scope, but with condition_codes and medication_codes removed

Coarsening active:

    Amount of patient files: 100

        Raw:

            1:
                parameters: {}

                released:
                    count: 100
                    all: 100
                    prevalence: 1.0
                raw:
                    count: 100
                    all: 100
                    prevalence: 1.0

                Preserved

            2:
                parameters: {   "gender": "female" } 

                inconclusive due to too small cohort size

            3: 
                parameters: {   "gender": "male" } 

                inconclusive due to too small cohort size

            4:
                parameters: {   "gender": "female",   "condition_codes": [     "314529007"   ] }

                inconclusive due to too small cohort size

            5: 
                parameters: {   "gender": "male",   "condition_codes": [     "73595000"   ] } 

                inconclusive due to too small cohort size

            6:
                parameters: {   "min_age": 20,   "medication_codes": [     "308136"   ] } 

                inconclusive due to too small cohort size

            7:
                parameters: {   "min_age": 40,   "max_age": 89,   "medication_codes": [     "106892",     "314076",     "310798"   ] } 

                inconclusive due to too small cohort size

            8: 
                parameters: {   "condition_codes": [     "160903007",     "160904001"   ],   "medication_codes": [     "308136"   ] } 

                inconclusive due to too small cohort size

            9: 
                parameters: {   "condition_codes": [     "314529007",     "66383009"   ] } 

                released:
                    count: 100
                    all: 100
                    prevalence: 1.0
                raw:
                    count: 100
                    all: 100
                    prevalence: 1.0

                Preserved

            10:
                parameters: {   "gender": "male",   "condition_codes": [     "423315002"   ],   "medication_codes": [     "860975"   ] } 

                inconclusive due to too small cohort size

        DP (Seed 42):

            1:
                parameters: {}

                released: (Epsilon: 0.5 | 1 | 2.5)
                    count: 100.21830974832301 | 100.1091548741615 | 100.0436619496646
                    all: 100.35729438796298 | 100.17864719398149 | 100.07145887759259
                    prevalence: 0.9986151017672649 | 0.9993063160487142 | 0.999722229212607
                raw:
                    count: 100
                    all: 100
                    prevalence: 1.0

                Preserved

            2:
                parameters: {   "gender": "female" } 

                inconclusive due to too small cohort size

            3: 
                parameters: {   "gender": "male" } 

                inconclusive due to too small cohort size

            4:
                parameters: {   "gender": "female",   "condition_codes": [     "314529007"   ] }

                inconclusive due to too small cohort size

            5: 
                parameters: {   "gender": "male",   "condition_codes": [     "73595000"   ] } 

                inconclusive due to too small cohort size

            6:
                parameters: {   "min_age": 20,   "medication_codes": [     "308136"   ] } 

                inconclusive due to too small cohort size

            7:
                parameters: {   "min_age": 40,   "max_age": 89,   "medication_codes": [     "106892",     "314076",     "310798"   ] } 

                inconclusive due to too small cohort size

            8: 
                parameters: {   "condition_codes": [     "160903007",     "160904001"   ],   "medication_codes": [     "308136"   ] } 

                inconclusive due to too small cohort size

            9: 
                parameters: {   "condition_codes": [     "314529007",     "66383009"   ] } 

                released: (Epsilon: 0.5 | 1 | 2.5)
                    count: 100.21830974832301 | 100.1091548741615 | 100.0436619496646
                    all: 100.35729438796298 | 100.17864719398149 | 100.07145887759259
                    prevalence: 0.9986151017672649 | 0.9993063160487142 | 0.999722229212607
                raw:
                    count: 100
                    all: 100
                    prevalence: 1.0

                Preserved

            10:
                parameters: {   "gender": "male",   "condition_codes": [     "423315002"   ],   "medication_codes": [     "860975"   ] } 

                inconclusive due to too small cohort size

        DP (Seed 5):

            1:
                parameters: {}

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 82.70078964380991 | 91.35039482190496 | 96.54015792876199
                    all: 82.70078964380991 | 91.35039482190496 | 96.54015792876199
                    prevalence: 1.0 | 1.0 | 1.0
                raw:
                    count: 100
                    all: 100
                    prevalence: 1.0

                Preserved

            2:
                parameters: {   "gender": "female" } 

                inconclusive due to too small cohort size

            3: 
                parameters: {   "gender": "male" } 

                inconclusive due to too small cohort size

            4:
                parameters: {   "gender": "female",   "condition_codes": [     "314529007"   ] }

                inconclusive due to too small cohort size

            5: 
                parameters: {   "gender": "male",   "condition_codes": [     "73595000"   ] } 

                inconclusive due to too small cohort size

            6:
                parameters: {   "min_age": 20,   "medication_codes": [     "308136"   ] } 

                inconclusive due to too small cohort size

            7:
                parameters: {   "min_age": 40,   "max_age": 89,   "medication_codes": [     "106892",     "314076",     "310798"   ] } 

                inconclusive due to too small cohort size

            8: 
                parameters: {   "condition_codes": [     "160903007",     "160904001"   ],   "medication_codes": [     "308136"   ] } 

                inconclusive due to too small cohort size

            9: 
                parameters: {   "condition_codes": [     "314529007",     "66383009"   ] } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 82.70078964380991 | 91.35039482190496 | 96.54015792876199
                    all: 82.70078964380991 | 91.35039482190496 | 96.54015792876199
                    prevalence: 1.0 | 1.0 | 1.0 
                raw:
                    count: 100
                    all: 100
                    prevalence: 1.0

                Preserved

            10:
                parameters: {   "gender": "male",   "condition_codes": [     "423315002"   ],   "medication_codes": [     "860975"   ] } 

                inconclusive due to too small cohort size

    Amount of patient files: 500

        Raw:

            1:
                parameters: {}

                released:
                    count: 500
                    all: 500
                    prevalence: 1.0
                raw:
                    count: 500
                    all: 500
                    prevalence: 1.0

                Preserved

            2:
                parameters: {   "gender": "female" } 

                released:
                    count: 243
                    all: 243
                    prevalence: 1.0
                raw:
                    count: 243
                    all: 243
                    prevalence: 1.0

                Preserved   

            3: 
                parameters: {   "gender": "male" } 

                released:
                    count: 257
                    all: 257
                    prevalence: 1.0
                raw:
                    count: 257
                    all: 257
                    prevalence: 1.0

                Preserved

            4:
                parameters: {   "gender": "female",   "condition_codes": [     "314529007"   ] }

                released:
                    count: 243
                    all: 243
                    prevalence: 1.0
                raw:
                    count: 243
                    all: 243
                    prevalence: 1.0

                Preserved

            5: 
                parameters: {   "gender": "male",   "condition_codes": [     "73595000"   ] } 

                released:
                    count: 214
                    all: 257
                    prevalence: 0.8326848249027238
                raw:
                    count: 214
                    all: 257
                    prevalence: 0.8326848249027238

                Preserved

            6:
                parameters: {   "min_age": 20,   "medication_codes": [     "308136"   ] } 

                inconclusive due to too small cohort size

            7:
                parameters: {   "min_age": 40,   "max_age": 89,   "medication_codes": [     "106892",     "314076",     "310798"   ] } 

                inconclusive due to too small cohort size

            8: 
                parameters: {   "condition_codes": [     "160903007",     "160904001"   ],   "medication_codes": [     "308136"   ] } 

                inconclusive due to too small cohort size

            9: 
                parameters: {   "condition_codes": [     "314529007",     "66383009"   ] } 

                released:
                    count: 500
                    all: 500
                    prevalence: 1.0
                raw:
                    count: 500
                    all: 500
                    prevalence: 1.0

                Preserved

            10:
                parameters: {   "gender": "male",   "condition_codes": [     "423315002"   ],   "medication_codes": [     "860975"   ] } 

                inconclusive due to too small cohort size

        DP (Seed 42):

            1:
                parameters: {}

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 500.218309748323 | 500.1091548741615 | 500.0436619496646
                    all: 500.357294387963 | 500.1786471939815 | 500.0714588775926
                    prevalence: 0.999722229212607 | 0.9998610650010554 | 0.9999444140883578
                raw:
                    count: 500
                    all: 500
                    prevalence: 1.0

                Preserved

            2:
                parameters: {   "gender": "female" } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 243.218309748323 | 243.1091548741615 | 243.0436619496646
                    all: 243.35729438796298 | 243.1786471939815 | 243.0714588775926
                    prevalence: 0.9994288864856526 | 0.9997142334632507 | 0.9998856429789974
                raw:
                    count: 243
                    all: 243
                    prevalence: 1.0

                Preserved   

            3: 
                parameters: {   "gender": "male" } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 257.218309748323 | 257.1091548741615 | 257.0436619496646
                    all: 257.357294387963 | 257.1786471939815 | 257.0714588775926
                    prevalence: 0.9994599545353066 | 0.9997297896984132 | 0.9998918708126942
                raw:
                    count: 257
                    all: 257
                    prevalence: 1.0

                Preserved

            4:
                parameters: {   "gender": "female",   "condition_codes": [     "314529007"   ] }

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 243.218309748323 | 243.1091548741615 | 243.0436619496646
                    all: 243.35729438796298 | 243.1786471939815 | 243.0714588775926
                    prevalence: 0.9994288864856526 | 0.9997142334632507 | 0.9998856429789974
                raw:
                    count: 243
                    all: 243
                    prevalence: 1.0

                Preserved

            5: 
                parameters: {   "gender": "male",   "condition_codes": [     "73595000"   ] } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 214.218309748323 | 214.1091548741615 | 214.0436619496646
                    all: 257.357294387963 | 257.1786471939815 | 257.0714588775926
                    prevalence: 0.8323770665127972 | 0.8325308388167465 | 0.8326232047859652
                raw:
                    count: 214
                    all: 257
                    prevalence: 0.8326848249027238

                Preserved

            6:
                parameters: {   "min_age": 20,   "medication_codes": [     "308136"   ] } 

                inconclusive due to too small cohort size

            7:
                parameters: {   "min_age": 40,   "max_age": 89,   "medication_codes": [     "106892",     "314076",     "310798"   ] } 

                inconclusive due to too small cohort size

            8: 
                parameters: {   "condition_codes": [     "160903007",     "160904001"   ],   "medication_codes": [     "308136"   ] } 

                inconclusive due to too small cohort size

            9: 
                parameters: {   "condition_codes": [     "314529007",     "66383009"   ] } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 500.218309748323 | 500.1091548741615 | 500.0436619496646
                    all: 500.357294387963 | 500.1786471939815 | 500.0714588775926
                    prevalence: 0.999722229212607 | 0.9998610650010554 | 0.9999444140883578
                raw:
                    count: 500
                    all: 500
                    prevalence: 1.0

                Preserved

            10:
                parameters: {   "gender": "male",   "condition_codes": [     "423315002"   ],   "medication_codes": [     "860975"   ] } 

                inconclusive due to too small cohort size

        DP (Seed 5):

            1:
                parameters: {}

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 482.7007896438099 | 491.35039482190496 | 496.54015792876197
                    all: 482.7007896438099 | 491.35039482190496 | 496.54015792876197
                    prevalence: 1.0 | 1.0 | 1.0
                raw:
                    count: 500
                    all: 500
                    prevalence: 1.0

                Preserved

            2:
                parameters: {   "gender": "female" } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 225.7007896438099 | 234.35039482190496 | 239.54015792876197
                    all: 225.7007896438099 | 234.35039482190496 | 239.54015792876197
                    prevalence: 1.0 | 1.0 | 1.0
                raw:
                    count: 243
                    all: 243
                    prevalence: 1.0

                Preserved   

            3: 
                parameters: {   "gender": "male" } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 239.7007896438099 | 248.35039482190496 | 253.54015792876197
                    all: 239.7007896438099 | 248.35039482190496 | 253.54015792876197
                    prevalence: 1.0 | 1.0 | 1.0
                raw:
                    count: 257
                    all: 257
                    prevalence: 1.0

                Preserved

            4:
                parameters: {   "gender": "female",   "condition_codes": [     "314529007"   ] }

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 225.7007896438099 | 234.35039482190496 | 239.54015792876197
                    all: 225.7007896438099 | 234.35039482190496 | 239.54015792876197
                    prevalence: 1.0 | 1.0 | 1.0
                raw:
                    count: 243
                    all: 243
                    prevalence: 1.0

                Preserved

            5: 
                parameters: {   "gender": "male",   "condition_codes": [     "73595000"   ] } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 212.48228970587115 | 213.24114485293558 | 213.69645794117423
                    all: 239.7007896438099 | 248.35039482190496 | 253.54015792876197
                    prevalence: 0.8864480172201984 | 0.8586301825928376 | 0.8428505357372903
                raw:
                    count: 214
                    all: 257
                    prevalence: 0.8326848249027238

                Preserved (5% off...) | (2% off) | 1%

            6:
                parameters: {   "min_age": 20,   "medication_codes": [     "308136"   ] } 

                inconclusive due to too small cohort size

            7:
                parameters: {   "min_age": 40,   "max_age": 89,   "medication_codes": [     "106892",     "314076",     "310798"   ] } 

                inconclusive due to too small cohort size

            8: 
                parameters: {   "condition_codes": [     "160903007",     "160904001"   ],   "medication_codes": [     "308136"   ] } 

                inconclusive due to too small cohort size

            9: 
                parameters: {   "condition_codes": [     "314529007",     "66383009"   ] } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 482.7007896438099 | 491.35039482190496 | 496.54015792876197
                    all: 482.7007896438099 | 491.35039482190496 | 496.54015792876197
                    prevalence: 1.0 | 1.0 | 1.0
                raw:
                    count: 500
                    all: 500
                    prevalence: 1.0

                Preserved

            10:
                parameters: {   "gender": "male",   "condition_codes": [     "423315002"   ],   "medication_codes": [     "860975"   ] } 

                inconclusive due to too small cohort size

    Amount of patient files: 2500

        Raw:

            1:
                parameters: {}

                released:
                    count: 2500
                    all: 2500
                    prevalence: 1.0
                raw:
                    count: 2500
                    all: 2500
                    prevalence: 1.0

                Preserved

            2:
                parameters: {   "gender": "female" } 

                released 
                    count: 1280
                    all: 1280
                    prevalence: 1.0
                raw:
                    count: 1280
                    all: 1280
                    prevalence: 1.0

                Preserved   

            3: 
                parameters: {   "gender": "male" } 

                released:
                    count: 1220
                    all: 1220
                    prevalence: 1.0
                raw:
                    count: 1220
                    all: 1220
                    prevalence: 1.0

                Preserved

            4:
                parameters: {   "gender": "female",   "condition_codes": [     "314529007"   ] }

                released 
                    count: 1280
                    all: 1280
                    prevalence: 1.0
                raw:
                    count: 1280
                    all: 1280
                    prevalence: 1.0

                Preserved

            5: 
                parameters: {   "gender": "male",   "condition_codes": [     "73595000"   ] } 

                released
                    count: 962
                    all: 1220
                    prevalence: 0.7885245901639344
                raw:
                    count: 962
                    all: 1220
                    prevalence: 0.7885245901639344

                Preserved

            6: 
                parameters: {   "min_age": 20,   "medication_codes": [     "308136"   ] } 

                released
                    count: 317
                    all: 1932
                    prevalence: 0.16407867494824016
                raw:
                    count: 317
                    all: 1984
                    prevalence: 0.15977822580645162

                Preserved (0.004300 bc of coarsening)

            7: 
                parameters: {   "min_age": 40,   "max_age": 89,   "medication_codes": [     "106892",     "314076",     "310798"   ] } 

                released
                    count: 446
                    all: 1218
                    prevalence: 0.36617405582922824
                raw:
                    count: 450
                    all: 1268
                    prevalence: 0.3548895899053628

                Preserved (0.011284)

            8: 
                parameters: {   "condition_codes": [     "160903007",     "160904001"   ],   "medication_codes": [     "308136"   ] }

                released
                    count: 317
                    all: 2500
                    prevalence: 0.1268
                raw:
                    count: 317
                    all: 2500
                    prevalence: 0.1268

                Preserved

            9: 
                parameters: {   "condition_codes": [     "314529007",     "66383009"   ] } 

                released
                    count: 2500
                    all: 2500
                    prevalence: 1.0
                raw:
                    count: 2500
                    all: 2500
                    prevalence: 1.0

                Preserved

            10:
                parameters: {   "gender": "male",   "condition_codes": [     "423315002"   ],   "medication_codes": [     "860975"   ] } 

                inconclusive due to too small cohort size

        DP (Seed 42):

            1:
                parameters: {}

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 2500.218309748323 | 2500.1091548741615 | 2500.0436619496645
                    all: 2500.357294387963 | 2500.1786471939813 | 2500.0714588775927
                    prevalence: 0.9999444140883577 | 0.9999722050582674 | 0.9999888815466336
                raw:
                    count: 2500
                    all: 2500
                    prevalence: 1.0

                Preserved

            2:
                parameters: {   "gender": "female" } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 1280.218309748323 | 1280.1091548741615 | 1280.0436619496645
                    all: 1280.357294387963 | 1280.1786471939815 | 1280.0714588775927
                    prevalence: 0.9998914485509246 | 0.9999457167013586 | 0.9999782848623525
                raw:
                    count: 1280
                    all: 1280   
                    prevalence: 1.0

                Preserved   

            3: 
                parameters: {   "gender": "male" } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 1220.218309748323 | 1220.1091548741615 | 1220.0436619496645
                    all: 1220.357294387963 | 1220.1786471939815 | 1220.0714588775927
                    prevalence: 0.9998861115180946 | 0.9999430474218018 | 0.9999772169673129
                raw:
                    count: 1220
                    all: 1220
                    prevalence: 1.0

                Preserved

            4:
                parameters: {   "gender": "female",   "condition_codes": [     "314529007"   ] }

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 1280.218309748323 | 1280.1091548741615 | 1280.0436619496645
                    all: 1280.357294387963 | 1280.1786471939815 | 1280.0714588775927
                    prevalence: 0.9998914485509246 | 0.9999457167013586 | 0.9999782848623525
                raw:
                    count: 1280
                    all: 1280
                    prevalence: 1.0

                Preserved

            5: 
                parameters: {   "gender": "male",   "condition_codes": [     "73595000"   ] } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 962.218309748323 | 962.1091548741615 | 962.0436619496646
                    all: 1220.357294387963 | 1220.1786471939815 | 1220.0714588775927
                    prevalence: 0.7884726171370142 | 0.7884985998457711 | 0.7885141931233264
                raw:
                    count: 962
                    all: 1220
                    prevalence: 0.7885245901639344

                Preserved

            6: 
                parameters: {   "min_age": 20,   "medication_codes": [     "308136"   ] } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 317.218309748323 | 317.1091548741615 | 317.0436619496646
                    all: 1932.357294387963 | 1932.1786471939815 | 1932.0714588775927
                    prevalence: 0.16416131254276958 | 0.16411999756579718 | 0.16409520491226876
                raw:
                    count: 317
                    all: 1984
                    prevalence: 0.15977822580645162

                Preserved (0.004300 bc of coarsening)

            7: 
                parameters: {   "min_age": 40,   "max_age": 89,   "medication_codes": [     "106892",     "314076",     "310798"   ] } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 446.218309748323 | 446.1091548741615 | 446.0436619496646
                    all: 1218.357294387963 | 1218.1786471939815 | 1218.0714588775927
                    prevalence: 0.36624585563176604 | 0.3662099609952558 | 0.36618841915947786
                raw:
                    count: 450
                    all: 1268
                    prevalence: 0.3548895899053628

                Preserved (0.011284)

            8: 
                parameters: {   "condition_codes": [     "160903007",     "160904001"   ],   "medication_codes": [     "308136"   ] }

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 317.218309748323 | 317.1091548741615 | 317.0436619496646
                    all: 2500.357294387963 | 2500.1786471939813 | 2500.0714588775927
                    prevalence: 0.1268691920392008 | 0.1268345984916165 | 0.1268138399899983
                raw:
                    count: 317
                    all: 2500
                    prevalence: 0.1268

                Preserved

            9: 
                parameters: {   "condition_codes": [     "314529007",     "66383009"   ] } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 2500.218309748323 | 2500.1091548741615 | 2500.0436619496645
                    all: 2500.357294387963 | 2500.1786471939813 | 2500.0714588775927
                    prevalence: 0.9999444140883577 | 0.9999722050582674 | 0.9999888815466336
                raw:
                    count: 2500
                    all: 2500
                    prevalence: 1.0

                Preserved

            10:
                parameters: {   "gender": "male",   "condition_codes": [     "423315002"   ],   "medication_codes": [     "860975"   ] } 

                inconclusive due to too small cohort size

        DP (Seed 5):

            1:
                parameters: {}

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 2482.70078964381 | 2491.350394821905 | 2496.540157928762
                    all: 2482.70078964381 | 2491.350394821905 | 2496.540157928762
                    prevalence: 1.0 | 1.0 | 1.0
                raw:
                    count: 2500
                    all: 2500
                    prevalence: 1.0

                Preserved

            2:
                parameters: {   "gender": "female" } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 1262.70078964381 | 1271.350394821905 | 1276.540157928762
                    all: 1262.70078964381 | 1271.350394821905 | 1276.540157928762
                    prevalence: 1.0 | 1.0 | 1.0
                raw:
                    count: 1280
                    all: 1280   
                    prevalence: 1.0

                Preserved   

            3: 
                parameters: {   "gender": "male" } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 1202.70078964381 | 1211.350394821905 | 1216.540157928762
                    all: 1202.70078964381 | 1211.350394821905 | 1216.540157928762
                    prevalence: 1.0 | 1.0 | 1.0
                raw:
                    count: 1220
                    all: 1220
                    prevalence: 1.0

                Preserved

            4:
                parameters: {   "gender": "female",   "condition_codes": [     "314529007"   ] }

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 1262.70078964381 | 1271.350394821905 | 1276.540157928762
                    all: 1262.70078964381 | 1271.350394821905 | 1276.540157928762
                    prevalence: 1.0 | 1.0 | 1.0
                raw:
                    count: 1280
                    all: 1280
                    prevalence: 1.0

                Preserved

            5: 
                parameters: {   "gender": "male",   "condition_codes": [     "73595000"   ] } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 960.4822897058712 | 961.2411448529356 | 961.6964579411742
                    all: 1202.70078964381 | 1211.350394821905 | 1216.540157928762
                    prevalence: 0.7986045224018903 | 0.793528568580901 | 0.7905176427373548
                raw:
                    count: 962
                    all: 1220
                    prevalence: 0.7885245901639344

                Preserved

            6: 
                parameters: {   "min_age": 20,   "medication_codes": [     "308136"   ] } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 315.48228970587115 | 316.2411448529356 | 316.69645794117423
                    all: 1914.70078964381 | 1923.350394821905 | 1928.540157928762
                    prevalence: 0.16476845437796056 | 0.16442201364053496 | 0.16421564085100718
                raw:
                    count: 317
                    all: 1984
                    prevalence: 0.15977822580645162

                Preserved

            7: 
                parameters: {   "min_age": 40,   "max_age": 89,   "medication_codes": [     "106892",     "314076",     "310798"   ] } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 444.48228970587115 | 445.2411448529356 | 445.69645794117423
                    all: 1200.70078964381 | 1209.350394821905 | 1214.540157928762
                    prevalence: 0.37018572282086 | 0.3681655430546281 | 0.36696724684777055
                raw:
                    count: 450
                    all: 1268
                    prevalence: 0.3548895899053628

                Preserved

            8: 
                parameters: {   "condition_codes": [     "160903007",     "160904001"   ],   "medication_codes": [     "308136"   ] }

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 315.48228970587115 | 316.2411448529356 | 316.69645794117423
                    all: 2482.70078964381 | 2491.350394821905 | 2496.540157928762
                    prevalence: 0.12707221547673211 | 0.12693563519215137 | 0.12685414129445424
                raw:
                    count: 317
                    all: 2500
                    prevalence: 0.1268

                Preserved

            9: 
                parameters: {   "condition_codes": [     "314529007",     "66383009"   ] } 

                released (Epsilon: 0.5 | 1 | 2.5)
                    count: 2482.70078964381 | 2491.350394821905 | 2496.540157928762
                    all: 2482.70078964381 | 2491.350394821905 | 2496.540157928762
                    prevalence: 1.0 | 1.0 | 1.0
                raw:
                    count: 2500
                    all: 2500
                    prevalence: 1.0

                Preserved

            10:
                parameters: {   "gender": "male",   "condition_codes": [     "423315002"   ],   "medication_codes": [     "860975"   ] } 

                inconclusive due to too small cohort size

    Amount of patient files: 6281

        Raw:

            1:
                parameters: {}

                released:
                    count: 2500
                    all: 2500
                    prevalence: 1.0
                raw:
                    count: 2500
                    all: 2500
                    prevalence: 1.0

                Preserved

            2:
                parameters: {   "gender": "female" } 

                released 
                    count: 1280
                    all: 1280
                    prevalence: 1.0
                raw:
                    count: 1280
                    all: 1280
                    prevalence: 1.0

                Preserved   

            3: 
                parameters: {   "gender": "male" } 

                released:
                    count: 1220
                    all: 1220
                    prevalence: 1.0
                raw:
                    count: 1220
                    all: 1220
                    prevalence: 1.0

                Preserved

            4:
                parameters: {   "gender": "female",   "condition_codes": [     "314529007"   ] }

                released 
                    count: 1280
                    all: 1280
                    prevalence: 1.0
                raw:
                    count: 1280
                    all: 1280
                    prevalence: 1.0

                Preserved

            5: 
                parameters: {   "gender": "male",   "condition_codes": [     "73595000"   ] } 

                released
                    count: 962
                    all: 1220
                    prevalence: 0.7885245901639344
                raw:
                    count: 962
                    all: 1220
                    prevalence: 0.7885245901639344

                Preserved

            6: 
                parameters: {   "min_age": 20,   "medication_codes": [     "308136"   ] } 

                released
                    count: 317
                    all: 1932
                    prevalence: 0.16407867494824016
                raw:
                    count: 317
                    all: 1984
                    prevalence: 0.15977822580645162

                Preserved (0.004300 bc of coarsening)

            7: 
                parameters: {   "min_age": 40,   "max_age": 89,   "medication_codes": [     "106892",     "314076",     "310798"   ] } 

                released
                    count: 446
                    all: 1218
                    prevalence: 0.36617405582922824
                raw:
                    count: 450
                    all: 1268
                    prevalence: 0.3548895899053628

                Preserved (0.011284)

            8: 
                parameters: {   "condition_codes": [     "160903007",     "160904001"   ],   "medication_codes": [     "308136"   ] }

                released
                    count: 317
                    all: 2500
                    prevalence: 0.1268
                raw:
                    count: 317
                    all: 2500
                    prevalence: 0.1268

                Preserved

            9: 
                parameters: {   "condition_codes": [     "314529007",     "66383009"   ] } 

                released
                    count: 2500
                    all: 2500
                    prevalence: 1.0
                raw:
                    count: 2500
                    all: 2500
                    prevalence: 1.0

                Preserved

            10:
                parameters: {   "gender": "male",   "condition_codes": [     "423315002"   ],   "medication_codes": [     "860975"   ] } 

                inconclusive due to too small cohort size



        

            




        






