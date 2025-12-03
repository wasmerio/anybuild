import streamlit as st
from langchain_core.messages import HumanMessage
from langchain_openai import ChatOpenAI

st.set_page_config(page_title="🦜🔗 Quickstart App")
st.title("🦜🔗 Quickstart App")

openai_api_key = st.sidebar.text_input("OpenAI API Key")


def generate_response(input_text):
    llm = ChatOpenAI(
        api_key=openai_api_key,
        model="gpt-5-nano",  # or "gpt-4"
    )

    response = llm.invoke([HumanMessage(content=input_text)])
    st.markdown("**Response:**")
    st.write(response.content)


with st.form("my_form"):
    text = st.text_area(
        "Enter text:",
        "What are the three key pieces of advice for learning how to code?",
    )
    submitted = st.form_submit_button("Submit")
    if not openai_api_key.startswith("sk-"):
        st.warning("Please enter your OpenAI API key!", icon="⚠")
    if submitted and openai_api_key.startswith("sk-"):
        generate_response(text)
