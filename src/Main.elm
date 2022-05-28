port module Main exposing (..)

import Browser
import Html exposing (..)
import Html.Attributes exposing (..)
import Html.Events exposing (onClick)
import Platform.Sub exposing (batch)

-- PORTS


port invoke : ( String, Maybe Language ) -> Cmd msg


port getVotes : (List Vote -> msg) -> Sub msg


port getLanguages : (List Language -> msg) -> Sub msg



-- MAIN


type alias Candidate =
    { name : String
    , pictureURL : String
    , votes : Int
    , description : String
    , color : String
    }


type alias Vote =
    { name : String
    }


type alias Language =
    { name : String
    }


type alias Languages =
    List Language


main : Program () Model Msg
main =
    Browser.element
        { init = init
        , update = update
        , view = view
        , subscriptions = subscriptions
        }



-- MODEL


type alias Model =
    { language : Maybe Language
    , votes : List Vote
    , languages : Languages
    }


init : () -> ( Model, Cmd Msg )
init _ =
    ( { language = Nothing
      , votes = []
      , languages = []
      }
    , Cmd.none
    )



-- UPDATE


type Msg
    = SubmitPost
    | ToggleLanguage Language
    | GetVotes (List Vote)
    | GetLanguages Languages


update : Msg -> Model -> ( Model, Cmd Msg )
update msg model =
    case msg of
        SubmitPost ->
            ( model, invoke ( "on_publish_vote", model.language ) )

        ToggleLanguage language ->
            ( { model | language = toggleLanguage language model }, Cmd.none )

        GetVotes votes ->
            ( { model | votes = votes }
            , Cmd.none
            )

        GetLanguages languages ->
            ( { model | languages = languages }
            , Cmd.none
            )



-- VIEW


view : Model -> Html Msg
view model =
    div
        []
        [ div [ class "intro" ]
            [ h1 []
                [ text "Vote for your favorite language!" ]
            , p [] [ text ("Total votes: " ++ (model.votes |> List.length |> String.fromInt)) ]
            ]
        , ul
            []
            (List.map
                (renderLanguage
                    model
                )
                model.languages
            )
        , button [ Html.Attributes.disabled (Nothing == model.language), onClick SubmitPost ] [ text "Vote!" ]
        , i
            [ class "background-icon"
            , class
                (case model.language of
                    Just language ->
                        "devicon-" ++ String.toLower language.name ++ "-plain fade-in"

                    Nothing ->
                        "fade-out"
                )
            ]
            []
        ]


toggleLanguage : Language -> Model -> Maybe Language
toggleLanguage language model =
    if Just language == model.language then
        Nothing

    else
        Just language


matchVoteByName : String -> Vote -> Bool
matchVoteByName name vote =
    vote.name == name


countLanguageVotes : Model -> Language -> Int
countLanguageVotes model language =
    List.length (List.filter (matchVoteByName language.name) model.votes)




languageMatch : Model -> Language -> Bool
languageMatch model currentLanguage =
    case model.language of
        Just language ->
            language.name == currentLanguage.name

        Nothing ->
            False


renderLanguage : Model -> Language -> Html Msg
renderLanguage model language =
    li
        [ onClick (ToggleLanguage language)
        , title (String.fromInt (countLanguageVotes model language) ++ " vote(s)")
        , attribute "style" ("--progress: " ++ (String.fromInt (round (toFloat (countLanguageVotes model language) / toFloat (List.length model.votes) * 100)))++"%")
        , class
            (if languageMatch model language then
                "picked"

             else
                ""
            )
        ]
        [ div [class "label"]
            [ span [] [ text language.name ]
            , i
                [ class ("devicon-" ++ String.toLower language.name ++ "-plain")
                ]
                []
            ]
        ]



-- SUBSCRIPTIONS


subscriptions : Model -> Sub Msg
subscriptions _ =
    batch
        [ getVotes
            GetVotes
        , getLanguages
            GetLanguages
        ]
